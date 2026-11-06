//! Obsidian pop-out windows.
//!
//! A pop-out is not a window of its own: Obsidian calls
//! `window.open('about:blank', '_blank', 'popup,…')` from the main page and
//! then builds the pop-out's document itself, so the main page needs the
//! returned `window` object. That only works when the new webview is created
//! as a child of the opener's webview (WebView2 `NewWindowRequested` →
//! `SetNewWindow`, WebKitGTK `create` → related view). Tauri does this when the
//! `on_new_window` handler returns [`NewWindowResponse::Create`] for a builder
//! that went through [`WebviewWindowBuilder::window_features`].
//!
//! `popout.js` is the page side. It runs in every Ignis window and turns
//! Obsidian's pop-out support back on, which Ignis switches off.
//!
//! Pop-outs belong to the window that opened them. They are named
//! `<owner>-popout-<n>`, are left out of the saved window list and are closed
//! with their owner. They are not the same thing as the extra windows from
//! *New window*, which get a workspace of their own.

use std::{
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    sync::Arc,
    time::Duration,
};

use tauri::{
    webview::{NewWindowFeatures, NewWindowResponse},
    AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use url::Url;

use crate::windows::{disable_browser_accelerator_keys, handle_new_window_request};

/// The page-side script that re-enables Obsidian's pop-outs.
pub const SCRIPT: &str = include_str!("popout.js");

const LABEL_INFIX: &str = "-popout-";

/// Used when the page does not ask for a size. WebKitGTK never reports one.
const DEFAULT_SIZE: (f64, f64) = (900.0, 700.0);
const MIN_SIZE: (f64, f64) = (320.0, 240.0);

/// How long a pop-out gets to close itself before it is closed for it.
const CLOSE_GRACE: Duration = Duration::from_secs(2);

static NEXT_POPOUT: AtomicU32 = AtomicU32::new(1);

/// Whether a window label belongs to a pop-out.
pub fn is_popout_label(label: &str) -> bool {
    label.contains(LABEL_INFIX)
}

/// The label of the main-style window that a pop-out hangs off. A pop-out
/// opened from a pop-out belongs to the same owner.
fn owner_of(label: &str) -> &str {
    label.split(LABEL_INFIX).next().unwrap_or(label)
}

/// The one place where the look of a pop-out window is decided. Windows keep
/// the native title bar for now.
fn frame_builder<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    builder.decorations(true)
}

/// Creates the window for a pop-out request from `opener` and hands it back
/// to the webview that asked. Runs inside the new-window callback, which is
/// where Tauri requires the window to be created.
pub fn create_window<R: Runtime>(
    app: &AppHandle<R>,
    opener: &str,
    features: NewWindowFeatures,
) -> NewWindowResponse<R> {
    let owner = owner_of(opener).to_owned();
    let label = format!(
        "{owner}{LABEL_INFIX}{}",
        NEXT_POPOUT.fetch_add(1, Ordering::Relaxed)
    );
    let (size, position) = (features.size(), features.position());
    let handle = app.clone();
    let opener_label = label.clone();

    let mut builder = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::External(Url::parse("about:blank").expect("about:blank is a valid URL")),
    )
    .window_features(features)
    .title("Ignis")
    .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
    // The page handles dropped files itself; see the main windows.
    .disable_drag_drop_handler()
    .on_document_title_changed(|window, title| {
        let _ = window.set_title(&title);
    })
    .on_new_window(move |requested, features| {
        handle_new_window_request(&handle, &opener_label, requested, features)
    });
    if size.is_none() {
        builder = builder.inner_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1);
    }
    if position.is_none() {
        builder = builder.center();
    }

    match frame_builder(builder).build() {
        Ok(window) => {
            disable_browser_accelerator_keys(&window);
            close_through_page(&window);
            #[cfg(any(target_os = "linux", windows))]
            close_with_page(&window);
            NewWindowResponse::Create { window }
        }
        Err(e) => {
            eprintln!("ignis-desktop: could not open a pop-out window: {e}");
            NewWindowResponse::Deny
        }
    }
}

/// Closes every pop-out that belongs to the window `owner`.
pub fn close_owned<R: Runtime>(app: &AppHandle<R>, owner: &str) {
    let prefix = format!("{owner}{LABEL_INFIX}");
    for (label, window) in app.webview_windows() {
        if label.starts_with(&prefix) {
            let _ = window.destroy();
        }
    }
}

/// Obsidian removes a pop-out's tabs from its layout when the pop-out's page
/// goes away through `beforeunload`. Destroying the native window skips that
/// and leaves the tabs in the saved layout, so the close button asks the page
/// to close itself first. The window is closed anyway if the page does not
/// manage that.
fn close_through_page<R: Runtime>(window: &WebviewWindow<R>) {
    let asked = Arc::new(AtomicBool::new(false));
    let target = window.clone();
    window.on_window_event(move |event| {
        let WindowEvent::CloseRequested { api, .. } = event else {
            return;
        };
        if asked.swap(true, Ordering::SeqCst) {
            return;
        }
        api.prevent_close();
        let _ = target.eval("window.close()");
        let target = target.clone();
        std::thread::spawn(move || {
            std::thread::sleep(CLOSE_GRACE);
            let _ = target.destroy();
        });
    });
}

/// When the page closes itself (`window.close()`, which Obsidian does for a
/// pop-out with no tabs left), wry removes only the webview: WebKitGTK destroys
/// the widget, WebView2 destroys the container window. The empty top-level
/// window would stay on screen. Close that window too.
#[cfg(target_os = "linux")]
fn close_with_page<R: Runtime>(window: &WebviewWindow<R>) {
    use gtk::prelude::WidgetExt;

    let target = window.clone();
    let result = window.with_webview(move |webview| {
        webview.inner().connect_destroy(move |_| {
            // Not on this thread: the window is being torn down right now.
            let target = target.clone();
            std::thread::spawn(move || {
                let _ = target.destroy();
            });
        });
    });
    if let Err(e) = result {
        eprintln!("ignis-desktop: pop-out windows will not close with their page: {e}");
    }
}

#[cfg(windows)]
fn close_with_page<R: Runtime>(window: &WebviewWindow<R>) {
    use webview2_com::WindowCloseRequestedEventHandler;

    let target = window.clone();
    let result = window.with_webview(move |webview| unsafe {
        let Ok(core) = webview.controller().CoreWebView2() else {
            return;
        };
        let mut token = Default::default();
        let added = core.add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                // Not on this thread: the window is being torn down right now.
                let target = target.clone();
                std::thread::spawn(move || {
                    let _ = target.destroy();
                });
                Ok(())
            })),
            &mut token,
        );
        if let Err(e) = added {
            eprintln!("ignis-desktop: pop-out windows will not close with their page: {e}");
        }
    });
    if let Err(e) = result {
        eprintln!("ignis-desktop: pop-out windows will not close with their page: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_identify_popouts_and_owners() {
        assert!(is_popout_label("main-popout-1"));
        assert!(is_popout_label("w2-popout-7"));
        assert!(!is_popout_label("main"));
        assert!(!is_popout_label("w2"));
        assert!(!is_popout_label("setup"));
        assert_eq!(owner_of("w2-popout-7"), "w2");
        assert_eq!(owner_of("w2"), "w2");
    }
}
