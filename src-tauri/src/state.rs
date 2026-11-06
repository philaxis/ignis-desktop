use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, MutexGuard, PoisonError,
    },
    time::{Duration, Instant},
};

use crate::config::Config;

/// Geometry events fire continuously while a window is dragged or resized;
/// write to disk at most this often (the final state is flushed on close).
const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

/// Shared application state, managed by Tauri.
pub struct AppState {
    path: PathBuf,
    config: Mutex<Config>,
    last_flush: Mutex<Instant>,
    /// Set while the whole app is shutting down, so that windows closing as
    /// part of the quit stay in the saved window list and come back next time.
    pub quitting: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AppState {
    pub fn new(path: PathBuf, config: Config) -> Self {
        Self {
            path,
            config: Mutex::new(config),
            last_flush: Mutex::new(Instant::now()),
            quitting: AtomicBool::new(false),
        }
    }

    pub fn config(&self) -> MutexGuard<'_, Config> {
        lock(&self.config)
    }

    pub fn is_quitting(&self) -> bool {
        self.quitting.load(Ordering::SeqCst)
    }

    /// Writes the config to disk now.
    pub fn flush(&self) {
        let config = self.config().clone();
        if let Err(e) = config.save(&self.path) {
            eprintln!("ignis-desktop: could not save {}: {e}", self.path.display());
        }
        *lock(&self.last_flush) = Instant::now();
    }

    /// Writes the config unless it was written very recently.
    pub fn flush_throttled(&self) {
        if lock(&self.last_flush).elapsed() >= FLUSH_INTERVAL {
            self.flush();
        }
    }
}
