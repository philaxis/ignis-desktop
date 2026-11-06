//! Window lifecycle: creating vault windows, remembering their geometry,
//! restoring them on start, and turning page-initiated new windows into
//! native ones.

use tauri::{
    webview::{NewWindowFeatures, NewWindowResponse},
    AppHandle, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::{
    config::{is_on_screen, next_window_id, workspace_for, Geometry, Rect, MAIN_WINDOW_ID},
    popout,
    state::AppState,
    urls::{classify_new_window, normalize_server_url, window_url, NewWindowTarget},
};

/// Label of the local window that asks for the server address.
pub const SETUP_LABEL: &str = "setup";

const DEFAULT_SIZE: (f64, f64) = (1200.0, 800.0);

/// Runs in every Ignis window before the page's own scripts. Ctrl+Shift+N
/// opens a new window by calling `window.open`, which the native side turns
/// into a new native window; this needs no IPC access for the remote page.
/// Every other key is left alone and reaches the page.
const SHORTCUT_SCRIPT: &str = r#"
(() => {
  if (window.top !== window || window.opener) return;
  window.addEventListener('keydown', (event) => {
    const isN = event.code === 'KeyN' || event.key === 'n' || event.key === 'N';
    if (event.ctrlKey && event.shiftKey && !event.altKey && !event.metaKey && isN) {
      event.preventDefault();
      event.stopImmediatePropagation();
      window.open(location.origin + location.pathname + '?ignis-desktop=new-window', '_blank');
    }
  }, true);
})();
"#;

/// Optional overrides for a window's first page.
#[derive(Default)]
pub struct OpenOptions {
    pub vault: Option<String>,
    pub file: Option<String>,
    /// Where to put a window that has no saved geometry of its own.
    pub geometry: Option<Geometry>,
}

/// Distance between a new window and the one it was opened from.
const CASCADE_OFFSET: f64 = 32.0;

/// Geometry for a new window: the focused window's, shifted down and right so
/// the two do not sit exactly on top of each other.
fn cascaded_geometry<R: Runtime>(app: &AppHandle<R>) -> Option<Geometry> {
    let windows = vault_windows(app);
    let anchor = windows
        .iter()
        .find(|w| w.is_focused().unwrap_or(false))
        .or(windows.first())?;
    record_geometry(anchor);
    let g = app
        .state::<AppState>()
        .config()
        .geometry_of(anchor.label())?;
    Some(Geometry {
        x: g.x + CASCADE_OFFSET,
        y: g.y + CASCADE_OFFSET,
        maximized: false,
        ..g
    })
}

fn server_url(app: &AppHandle<impl Runtime>) -> Option<Url> {
    let state = app.state::<AppState>();
    let raw = state.config().server_url.clone()?;
    Url::parse(&raw).ok()
}

fn vault_windows<R: Runtime>(app: &AppHandle<R>) -> Vec<WebviewWindow<R>> {
    app.webview_windows()
        .into_iter()
        .filter(|(label, _)| label != SETUP_LABEL && !popout::is_popout_label(label))
        .map(|(_, window)| window)
        .collect()
}

fn monitor_rects<R: Runtime>(app: &AppHandle<R>) -> Vec<Rect> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let scale = m.scale_factor();
            let position = m.position().to_logical::<f64>(scale);
            let size = m.size().to_logical::<f64>(scale);
            Rect {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
            }
        })
        .collect()
}

/// Opens (or focuses) the window with the given id on the configured server.
pub fn open_window<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    options: OpenOptions,
) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(id) {
        let _ = existing.unminimize();
        let _ = existing.set_focus();
        return Ok(());
    }
    let server = server_url(app).ok_or("No server address is configured.")?;
    let state = app.state::<AppState>();
    let (vault, saved) = {
        let mut config = state.config();
        config.ensure_window(id);
        (
            config.vault.clone(),
            config.geometry_of(id).or(options.geometry),
        )
    };
    let vault = options.vault.or(vault);
    let url = window_url(
        &server,
        vault.as_deref(),
        workspace_for(id),
        options.file.as_deref(),
    );

    #[cfg(debug_assertions)]
    eprintln!("ignis-desktop: opening window {id}: {url}");

    let handle = app.clone();
    let label = id.to_owned();
    let mut builder = WebviewWindowBuilder::new(app, id, WebviewUrl::External(url))
        .title("Ignis")
        .inner_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1)
        .min_inner_size(420.0, 320.0)
        // Ignis uploads files by drag and drop; Tauri's own file-drop handler
        // would swallow those events on Windows.
        .disable_drag_drop_handler()
        .initialization_script(SHORTCUT_SCRIPT)
        .initialization_script(popout::SCRIPT)
        .on_document_title_changed(|window, title| {
            let _ = window.set_title(&title);
        })
        .on_new_window(move |requested, features| {
            handle_new_window_request(&handle, &label, requested, features)
        });

    match saved {
        Some(g) if is_on_screen(&g, &monitor_rects(app)) => {
            builder = builder
                .inner_size(g.width, g.height)
                .position(g.x, g.y)
                .maximized(g.maximized);
        }
        Some(g) => {
            builder = builder.inner_size(g.width, g.height).center();
        }
        None => builder = builder.center(),
    }

    let window = builder.build().map_err(|e| e.to_string())?;
    disable_browser_accelerator_keys(&window);
    track_window(app, &window);
    Ok(())
}

/// Opens a brand-new window with its own Ignis workspace.
pub fn open_new_window<R: Runtime>(app: &AppHandle<R>, mut options: OpenOptions) {
    if server_url(app).is_none() {
        open_setup_window(app);
        return;
    }
    let id = {
        let state = app.state::<AppState>();
        let config = state.config();
        next_window_id(&config.window_ids())
    };
    options.geometry = cascaded_geometry(app);
    if let Err(e) = open_window(app, &id, options) {
        eprintln!("ignis-desktop: could not open a new window: {e}");
    }
}

/// Reacts to `window.open` / `target=_blank` from the page of the window
/// `opener`. Runs on the UI thread. A pop-out has to be created right here
/// because the page is waiting for its `window`; everything else is moved to
/// another thread, since creating windows from within a UI-thread callback can
/// deadlock on Windows.
pub fn handle_new_window_request<R: Runtime>(
    app: &AppHandle<R>,
    opener: &str,
    requested: Url,
    features: NewWindowFeatures,
) -> NewWindowResponse<R> {
    let Some(server) = server_url(app) else {
        return NewWindowResponse::Deny;
    };
    let target = classify_new_window(&server, &requested);
    if target == NewWindowTarget::Popout {
        return popout::create_window(app, opener, features);
    }
    let app = app.clone();
    std::thread::spawn(move || match target {
        NewWindowTarget::Native { vault, file } => {
            open_new_window(
                &app,
                OpenOptions {
                    vault,
                    file,
                    geometry: None,
                },
            );
        }
        NewWindowTarget::External(url) => {
            if let Err(e) = app.opener().open_url(url.as_str(), None::<&str>) {
                eprintln!("ignis-desktop: could not open {url} in the browser: {e}");
            }
        }
        NewWindowTarget::Popout | NewWindowTarget::Ignore => {}
    });
    NewWindowResponse::Deny
}

/// Opens the windows from the last session, or the main window if there were
/// none.
pub fn restore_windows<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let mut ids = state.config().window_ids();
    if ids.is_empty() {
        ids.push(MAIN_WINDOW_ID.to_owned());
    }
    for id in ids {
        if let Err(e) = open_window(app, &id, OpenOptions::default()) {
            eprintln!("ignis-desktop: could not open window {id}: {e}");
        }
    }
}

/// Shows the server-address page, prefilled with the current settings.
pub fn open_setup_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(SETUP_LABEL) {
        let _ = window.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(app, SETUP_LABEL, WebviewUrl::App("index.html".into()))
        .title("Ignis Desktop: server address")
        .inner_size(480.0, 400.0)
        .resizable(false)
        .maximizable(false)
        .center()
        .build();
    if let Err(e) = result {
        eprintln!("ignis-desktop: could not open the settings window: {e}");
    }
}

/// Stores new settings and applies them: running windows are pointed at the
/// new server, and if none are open yet (first run) the windows are opened.
pub fn apply_settings<R: Runtime>(
    app: &AppHandle<R>,
    server_input: &str,
    vault_input: &str,
) -> Result<(), String> {
    let server = normalize_server_url(server_input)?;
    let vault = vault_input.trim();
    let state = app.state::<AppState>();
    {
        let mut config = state.config();
        config.server_url = Some(server.to_string());
        config.vault = (!vault.is_empty()).then(|| vault.to_owned());
    }
    state.flush();

    let open = vault_windows(app);
    if open.is_empty() {
        restore_windows(app);
    } else {
        let vault = state.config().vault.clone();
        for window in open {
            let url = window_url(
                &server,
                vault.as_deref(),
                workspace_for(window.label()),
                None,
            );
            window.navigate(url).map_err(|e| e.to_string())?;
        }
    }
    if let Some(setup) = app.get_webview_window(SETUP_LABEL) {
        let _ = setup.close();
    }
    Ok(())
}

/// Saves every window's geometry, then exits. Windows closed by the exit stay
/// in the saved list, so the next start brings them back.
pub fn quit<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    state
        .quitting
        .store(true, std::sync::atomic::Ordering::SeqCst);
    for window in vault_windows(app) {
        record_geometry(&window);
    }
    state.flush();
    app.exit(0);
}

fn capture_geometry<R: Runtime>(
    window: &WebviewWindow<R>,
    previous: Option<Geometry>,
) -> Option<Geometry> {
    if window.is_minimized().ok()? {
        return None;
    }
    let scale = window.scale_factor().ok()?;
    let maximized = window.is_maximized().ok()?;
    if maximized {
        if let Some(previous) = previous {
            // Keep the restored (non-maximized) size to return to.
            return Some(Geometry {
                maximized: true,
                ..previous
            });
        }
    }
    let position: PhysicalPosition<i32> = window.outer_position().ok()?;
    let position = position.to_logical::<f64>(scale);
    let size = window.inner_size().ok()?.to_logical::<f64>(scale);
    Some(Geometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized,
    })
}

fn record_geometry<R: Runtime>(window: &WebviewWindow<R>) {
    let state = window.state::<AppState>();
    let id = window.label();
    let previous = state.config().geometry_of(id);
    if let Some(geometry) = capture_geometry(window, previous) {
        state.config().set_geometry(id, geometry);
    }
}

fn track_window<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let handle = app.clone();
    let label = window.label().to_owned();
    window.on_window_event(move |event| {
        let Some(window) = handle.get_webview_window(&label) else {
            return;
        };
        let state = handle.state::<AppState>();
        match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                record_geometry(&window);
                state.flush_throttled();
            }
            WindowEvent::CloseRequested { .. } => {
                record_geometry(&window);
                // Closing one of several windows forgets it; the last window
                // (or a quit) keeps its record so it is restored next time.
                if !state.is_quitting() && vault_windows(&handle).len() > 1 {
                    state.config().remove_window(&label);
                }
                state.flush();
                popout::close_owned(&handle, &label);
            }
            _ => {}
        }
    });
}

/// WebView2 handles Ctrl+F, Ctrl+R, F5, Ctrl+P, Ctrl+U, F12 and friends
/// itself. Turning that off makes the page receive all of them.
#[cfg(windows)]
pub fn disable_browser_accelerator_keys<R: Runtime>(window: &WebviewWindow<R>) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use windows_core::Interface;

    let result = window.with_webview(|webview| unsafe {
        let settings = webview
            .controller()
            .CoreWebView2()
            .and_then(|core| core.Settings());
        match settings.and_then(|s| s.cast::<ICoreWebView2Settings3>()) {
            Ok(settings) => {
                if let Err(e) = settings.SetAreBrowserAcceleratorKeysEnabled(false) {
                    eprintln!("ignis-desktop: could not disable browser shortcuts: {e}");
                }
            }
            Err(e) => eprintln!("ignis-desktop: could not reach the WebView2 settings: {e}"),
        }
    });
    if let Err(e) = result {
        eprintln!("ignis-desktop: could not configure the webview: {e}");
    }
}

#[cfg(not(windows))]
pub fn disable_browser_accelerator_keys<R: Runtime>(_window: &WebviewWindow<R>) {}
