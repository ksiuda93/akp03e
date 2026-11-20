//! Driver for the Ajazz AKP03E (rev. 2, 0300:3002) with a browser-based configuration panel
//! and a system tray icon.

// The release build runs in the background without a console window; debug keeps the console for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod config;
mod device;
mod render;
mod state;
mod system;
mod tray;
mod web;

use std::{
    path::PathBuf,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicUsize},
        mpsc,
    },
    time::Duration,
};

use tokio::sync::{Notify, broadcast, watch};

use crate::{config::Config, state::AppState};

fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("akp03e")
}

fn fatal(msg: &str) -> ! {
    eprintln!("{msg}");
    // Without a console the message has to be shown some other way.
    let _ = std::process::Command::new("msg").args(["*", &format!("AKP03E: {msg}")]).spawn();
    std::process::exit(1);
}

fn main() {
    let from_autostart = std::env::args().any(|a| a == system::AUTOSTART_ARG);

    if !system::acquire_single_instance() {
        // Already running – show the panel instead of starting a second copy.
        if !from_autostart {
            system::open_panel();
        }
        return;
    }

    let dir = data_dir();
    let icons_dir = dir.join("icons");
    if let Err(e) = std::fs::create_dir_all(&icons_dir) {
        fatal(&format!("cannot create {}: {e}", icons_dir.display()));
    }
    let config_path = dir.join("config.toml");
    let config = Config::load_or_create(&config_path)
        .unwrap_or_else(|e| fatal(&format!("{e}\nFix the file, or delete it to get the default configuration.")));

    let (actions_tx, actions_rx) = mpsc::channel();
    let (quit_tx, _) = watch::channel(false);
    let state = Arc::new(AppState {
        config: RwLock::new(config),
        config_path: config_path.clone(),
        icons_dir,
        profile: AtomicUsize::new(0),
        screen_on: AtomicBool::new(true),
        mic_muted: AtomicBool::new(false),
        connected: AtomicBool::new(false),
        autostart: AtomicBool::new(system::refresh_autostart()),
        rerender: Notify::new(),
        reconnect: Notify::new(),
        events: broadcast::channel(256).0,
        actions: actions_tx,
        quit: quit_tx,
    });

    // The log of the previous run is not needed.
    let _ = std::fs::remove_file(dir.join("akp03e.log"));
    state.log("info", format!("Started. Config: {}, panel: http://{}", config_path.display(), web::ADDR));

    // A panic on any thread goes to the log – without a console it would otherwise vanish without a trace.
    let panic_state = state.clone();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        panic_state.log("error", format!("Panic: {info}"));
        default_hook(info);
    }));

    system::watch_display(state.clone());
    actions::spawn_worker(state.clone(), actions_rx);

    // Tokio runs on its own thread – the main thread handles the tray icon.
    let rt_state = state.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio");
        rt.block_on(async move {
            let device_task = tokio::spawn(device::run(rt_state.clone()));
            let web_task = tokio::spawn(web::serve(rt_state.clone()));

            let mut quit = rt_state.quit.subscribe();
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = quit.changed() => {}
            }
            rt_state.log("info", "Shutting down");
            let _ = rt_state.quit.send(true);
            // Only wait for the controller cleanup; a panel with an open event stream never closes by itself.
            let _ = tokio::time::timeout(Duration::from_secs(3), device_task).await;
            web_task.abort();
            // HID reads run on blocking threads that the runtime would wait for forever.
            std::process::exit(0);
        });
    });

    if !from_autostart {
        // Give the server a moment to start before opening the browser.
        let s = state.clone();
        std::thread::spawn(move || {
            for _ in 0..50 {
                if std::net::TcpStream::connect(web::ADDR).is_ok() {
                    system::open_panel();
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            s.log("error", "The panel did not start within 10 s");
        });
    }

    tray::run(state);
}
