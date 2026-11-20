//! Shared application state: configuration, active profile, event channels.

use std::{
    path::PathBuf,
    sync::{
        RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
};

use serde_json::{Value, json};
use tokio::sync::{Notify, broadcast, watch};

use crate::config::{Action, Config};

pub struct AppState {
    pub config: RwLock<Config>,
    pub config_path: PathBuf,
    pub icons_dir: PathBuf,
    pub profile: AtomicUsize,
    pub screen_on: AtomicBool,
    pub mic_muted: AtomicBool,
    pub connected: AtomicBool,
    pub autostart: AtomicBool,
    /// Request to redraw the keys on the device.
    pub rerender: Notify,
    /// Request to close and reopen the connection to the controller
    /// (after the PC wakes up, reading key presses can stop working).
    pub reconnect: Notify,
    /// Events for the panel (JSON), sent over SSE.
    pub events: broadcast::Sender<String>,
    /// Actions to run on the Windows worker thread.
    pub actions: mpsc::Sender<Action>,
    pub quit: watch::Sender<bool>,
}

impl AppState {
    pub fn config(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    pub fn emit(&self, value: Value) {
        let _ = self.events.send(value.to_string());
    }

    pub fn log(&self, level: &str, msg: impl Into<String>) {
        let msg = msg.into();
        let line = if level == "error" { format!("[error] {msg}") } else { msg.clone() };
        if cfg!(debug_assertions) {
            println!("{line}");
        } else {
            // The background build has no console (and inherited output can block) – log to a file.
            use std::io::Write;
            let path = self.config_path.with_file_name("akp03e.log");
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let _ = writeln!(f, "{secs} {line}");
            }
        }
        self.emit(json!({ "type": "log", "level": level, "msg": msg }));
    }

    pub fn status(&self) -> Value {
        json!({
            "type": "status",
            "connected": self.connected.load(Ordering::Relaxed),
            "profile": self.profile.load(Ordering::Relaxed),
            "screen_on": self.screen_on.load(Ordering::Relaxed),
            "mic_muted": self.mic_muted.load(Ordering::Relaxed),
            "autostart": self.autostart.load(Ordering::Relaxed),
        })
    }

    /// Writes the autostart entry to the registry; the in-memory flag reflects what actually succeeded.
    pub fn set_autostart(&self, enabled: bool) {
        match crate::system::set_autostart(enabled) {
            Ok(()) => {
                self.autostart.store(enabled, Ordering::Relaxed);
                self.log("info", if enabled { "Start with Windows enabled" } else { "Start with Windows disabled" });
            }
            Err(e) => self.log("error", format!("Start with Windows: {e}")),
        }
        self.emit_status();
    }

    pub fn emit_status(&self) {
        self.emit(self.status());
    }

    pub fn set_profile(&self, index: usize) {
        let count = self.config.read().unwrap().profiles.len();
        self.profile.store(index.min(count - 1), Ordering::Relaxed);
        self.rerender.notify_one();
        self.emit_status();
    }

    pub fn shift_profile(&self, delta: isize) {
        let count = self.config.read().unwrap().profiles.len() as isize;
        let current = self.profile.load(Ordering::Relaxed) as isize;
        self.set_profile((current + delta).rem_euclid(count) as usize);
    }
}
