//! Frameless windows. Without a native title bar, the page itself has to move,
//! maximize, minimize and close the window. `frame.js` draws the controls and
//! calls the `frame_*` commands below.
//!
//! Remote pages can only reach these commands through a runtime capability
//! that names the configured server's origin; see [`allow_server`]. Each
//! command also checks the caller's origin again, because a capability cannot
//! be removed once added: after the server address changes, the old origin's
//! capability stays until the app restarts.

use std::{collections::HashSet, sync::Mutex};

use serde::Serialize;
use tauri::{ipc::CapabilityBuilder, AppHandle, Manager, Runtime, WebviewWindow};
use url::Url;

use crate::{snap::SnapRect, state::AppState, urls::is_trusted_page};

const SCRIPT: &str = include_str!("frame.js");

/// Origins that already have a capability.
#[derive(Default)]
pub struct FrameAccess {
    origins: Mutex<HashSet<String>>,
}

/// The script that draws the window controls and handles dragging.
pub fn script() -> &'static str {
    SCRIPT
}

/// The capability pattern for one server origin, for example
/// `https://ignis.example.ts.net/*`.
fn origin_pattern(server: &Url) -> String {
    format!("{}/*", server.origin().ascii_serialization())
}

/// Lets pages from the server's origin, and only that origin, use the frame
/// commands. Safe to call again for an origin that is already allowed.
pub fn allow_server<R: Runtime>(app: &AppHandle<R>, server: &Url) {
    let pattern = origin_pattern(server);
    let access = app.state::<FrameAccess>();
    let mut origins = access
        .origins
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if origins.contains(&pattern) {
        return;
    }
    let capability = CapabilityBuilder::new(format!("remote-frame-{}", origins.len()))
        .local(false)
        .remote(pattern.clone())
        .window("*")
        .permission("frame-controls");
    match app.add_capability(capability) {
        Ok(()) => {
            origins.insert(pattern);
        }
        Err(e) => eprintln!("ignis-desktop: could not allow window controls for {server}: {e}"),
    }
}

/// Rejects calls from a page that is neither one of the app's own pages nor on
/// the configured server.
pub(crate) fn check_caller<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    let page = window.url().map_err(|e| e.to_string())?;
    let server = window
        .state::<AppState>()
        .config()
        .server_url
        .as_deref()
        .and_then(|s| Url::parse(s).ok());
    if is_trusted_page(server.as_ref(), &page) {
        Ok(())
    } else {
        Err("This page may not control the window.".into())
    }
}

#[derive(Serialize)]
pub struct FrameState {
    maximized: bool,
}

#[tauri::command]
pub fn frame_start_dragging<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    check_caller(&window)?;
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn frame_minimize<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    check_caller(&window)?;
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn frame_toggle_maximize<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    check_caller(&window)?;
    let result = if window.is_maximized().map_err(|e| e.to_string())? {
        window.unmaximize()
    } else {
        window.maximize()
    };
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn frame_close<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    check_caller(&window)?;
    window.close().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn frame_state<R: Runtime>(window: WebviewWindow<R>) -> Result<FrameState, String> {
    check_caller(&window)?;
    Ok(FrameState {
        maximized: window.is_maximized().map_err(|e| e.to_string())?,
    })
}

/// The page reports where its maximize button is (client pixels) so that
/// Windows can show the Snap Layouts flyout over it; `None` when there is no
/// such button. See `snap.rs`.
#[tauri::command]
pub fn frame_snap_rect<R: Runtime>(
    window: WebviewWindow<R>,
    rect: Option<SnapRect>,
) -> Result<(), String> {
    check_caller(&window)?;
    crate::snap::place(&window, rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::utils::acl::RemoteUrlPattern;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn origin_pattern_names_scheme_host_and_port() {
        assert_eq!(
            origin_pattern(&url("https://ignis.example.ts.net/")),
            "https://ignis.example.ts.net/*"
        );
        assert_eq!(
            origin_pattern(&url("http://127.0.0.1:8080/base/")),
            "http://127.0.0.1:8080/*"
        );
    }

    #[test]
    fn pattern_matches_only_the_server_origin() {
        let pattern: RemoteUrlPattern = origin_pattern(&url("http://127.0.0.1:8080/"))
            .parse()
            .unwrap();
        assert!(pattern.test(&url("http://127.0.0.1:8080/?vault=v&workspace=w2")));
        assert!(pattern.test(&url("http://127.0.0.1:8080/deep/path")));
        assert!(!pattern.test(&url("http://127.0.0.1:8081/")));
        assert!(!pattern.test(&url("https://127.0.0.1:8080/")));
        assert!(!pattern.test(&url("http://evil.example/?http://127.0.0.1:8080/")));
    }
}
