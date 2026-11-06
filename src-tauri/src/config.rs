//! Persistent settings: server address, vault, and the set of open windows
//! with their geometry. Stored as JSON in the app config directory.

use std::{fs, io, path::Path};

use serde::{Deserialize, Serialize};

/// Id of the first window. It loads the bare vault URL, without a
/// `workspace` parameter, so it keeps Ignis's default layout.
pub const MAIN_WINDOW_ID: &str = "main";

/// Window position and size in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowRecord {
    pub id: String,
    #[serde(default)]
    pub geometry: Option<Geometry>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server_url: Option<String>,
    pub vault: Option<String>,
    pub windows: Vec<WindowRecord>,
}

/// Window ids become webview labels and Ignis workspace names, so keep them
/// to a conservative alphabet.
fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Config {
    /// Loads the config. A missing file gives the default; an unreadable or
    /// corrupt file is an error so the caller can decide what to do.
    pub fn load(path: &Path) -> io::Result<Config> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(e) => return Err(e),
        };
        // Windows editors (and PowerShell's `Set-Content -Encoding UTF8`) add a BOM.
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let mut config: Config = serde_json::from_str(text)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        config.sanitize();
        Ok(config)
    }

    /// Writes the config atomically (temp file + rename).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        fs::write(&tmp, json)?;
        fs::rename(&tmp, path)
    }

    /// Drops invalid and duplicate window ids (the file may be hand-edited).
    fn sanitize(&mut self) {
        let mut seen = Vec::new();
        self.windows.retain(|w| {
            if is_valid_id(&w.id) && !seen.contains(&w.id) {
                seen.push(w.id.clone());
                true
            } else {
                false
            }
        });
    }

    pub fn window_ids(&self) -> Vec<String> {
        self.windows.iter().map(|w| w.id.clone()).collect()
    }

    pub fn geometry_of(&self, id: &str) -> Option<Geometry> {
        self.windows.iter().find(|w| w.id == id)?.geometry
    }

    /// Registers a window id if it is not there yet.
    pub fn ensure_window(&mut self, id: &str) {
        if !self.windows.iter().any(|w| w.id == id) {
            self.windows.push(WindowRecord {
                id: id.to_owned(),
                geometry: None,
            });
        }
    }

    pub fn set_geometry(&mut self, id: &str, geometry: Geometry) {
        self.ensure_window(id);
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            w.geometry = Some(geometry);
        }
    }

    pub fn remove_window(&mut self, id: &str) {
        self.windows.retain(|w| w.id != id);
    }
}

/// Picks the id for a new window: the lowest `wN` (N >= 2) not in use, so a
/// closed window's workspace is reused instead of piling up forever.
pub fn next_window_id(existing: &[String]) -> String {
    (2..)
        .map(|n| format!("w{n}"))
        .find(|id| !existing.contains(id))
        .expect("an unused window id always exists")
}

/// The Ignis workspace a window id maps to. The main window uses Ignis's
/// default workspace.
pub fn workspace_for(id: &str) -> Option<&str> {
    (id != MAIN_WINDOW_ID).then_some(id)
}

/// A rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Whether a saved window position is still usable: a meaningful part of the
/// window must overlap a connected monitor (monitors get unplugged).
pub fn is_on_screen(geometry: &Geometry, monitors: &[Rect]) -> bool {
    const MIN_OVERLAP_X: f64 = 120.0;
    const MIN_OVERLAP_Y: f64 = 60.0;
    monitors.iter().any(|m| {
        let overlap_x = (geometry.x + geometry.width).min(m.x + m.width) - geometry.x.max(m.x);
        let overlap_y = (geometry.y + geometry.height).min(m.y + m.height) - geometry.y.max(m.y);
        overlap_x >= MIN_OVERLAP_X && overlap_y >= MIN_OVERLAP_Y
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(x: f64, y: f64) -> Geometry {
        Geometry {
            x,
            y,
            width: 1000.0,
            height: 700.0,
            maximized: false,
        }
    }

    #[test]
    fn missing_file_loads_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(&dir.path().join("config.json")).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.json");
        let mut config = Config {
            server_url: Some("https://ignis.example.ts.net/".into()),
            vault: Some("vault".into()),
            ..Config::default()
        };
        config.ensure_window(MAIN_WINDOW_ID);
        config.set_geometry("w2", geometry(10.0, 20.0));
        config.save(&path).unwrap();

        assert_eq!(Config::load(&path).unwrap(), config);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn a_utf8_byte_order_mark_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "\u{feff}{\"vault\":\"v\"}").unwrap();
        assert_eq!(Config::load(&path).unwrap().vault.as_deref(), Some("v"));
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{ not json").unwrap();
        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{"server_url":"https://a.example/","future":1}"#).unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.server_url.as_deref(), Some("https://a.example/"));
        assert!(config.windows.is_empty());
    }

    #[test]
    fn load_drops_invalid_and_duplicate_window_ids() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(
            &path,
            r#"{"windows":[{"id":"main"},{"id":"main"},{"id":"bad id!"},{"id":""},{"id":"w2"}]}"#,
        )
        .unwrap();
        assert_eq!(Config::load(&path).unwrap().window_ids(), ["main", "w2"]);
    }

    #[test]
    fn window_records_can_be_updated_and_removed() {
        let mut config = Config::default();
        config.set_geometry("w2", geometry(1.0, 2.0));
        assert_eq!(config.geometry_of("w2"), Some(geometry(1.0, 2.0)));
        config.set_geometry("w2", geometry(5.0, 6.0));
        assert_eq!(config.windows.len(), 1);
        assert_eq!(config.geometry_of("w2"), Some(geometry(5.0, 6.0)));
        config.remove_window("w2");
        assert!(config.windows.is_empty());
        assert_eq!(config.geometry_of("w2"), None);
    }

    #[test]
    fn next_window_id_fills_gaps() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(next_window_id(&ids(&[])), "w2");
        assert_eq!(next_window_id(&ids(&["main"])), "w2");
        assert_eq!(next_window_id(&ids(&["main", "w2", "w3"])), "w4");
        assert_eq!(next_window_id(&ids(&["main", "w3"])), "w2");
    }

    #[test]
    fn only_non_main_windows_have_a_workspace() {
        assert_eq!(workspace_for(MAIN_WINDOW_ID), None);
        assert_eq!(workspace_for("w2"), Some("w2"));
    }

    #[test]
    fn on_screen_check_handles_unplugged_monitors() {
        let monitors = [Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        }];
        assert!(is_on_screen(&geometry(100.0, 100.0), &monitors));
        // Partly off the edge but still mostly visible.
        assert!(is_on_screen(&geometry(1500.0, 800.0), &monitors));
        // Saved on a second monitor that is gone now.
        assert!(!is_on_screen(&geometry(2500.0, 100.0), &monitors));
        assert!(!is_on_screen(&geometry(-1900.0, 100.0), &monitors));
        assert!(!is_on_screen(&geometry(100.0, 100.0), &[]));
    }
}
