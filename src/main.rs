//! Obsługa Ajazz AKP03E (rev. 2, 0300:3002) z panelem konfiguracyjnym w przeglądarce
//! i ikoną w zasobniku systemowym.

// Wersja release działa w tle bez okna konsoli; debug zostawia konsolę z logami.
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
    // W wersji bez konsoli komunikat musi być widoczny.
    let _ = std::process::Command::new("msg").args(["*", &format!("AKP03E: {msg}")]).spawn();
    std::process::exit(1);
}

fn main() {
    let from_autostart = std::env::args().any(|a| a == system::AUTOSTART_ARG);

    if !system::acquire_single_instance() {
        // Program już działa – pokaż panel zamiast uruchamiać drugą kopię.
        if !from_autostart {
            system::open_panel();
        }
        return;
    }

    let dir = data_dir();
    let icons_dir = dir.join("icons");
    if let Err(e) = std::fs::create_dir_all(&icons_dir) {
        fatal(&format!("nie można utworzyć {}: {e}", icons_dir.display()));
    }
    let config_path = dir.join("config.toml");
    let config = Config::load_or_create(&config_path)
        .unwrap_or_else(|e| fatal(&format!("{e}\nPopraw plik albo usuń go, żeby wygenerować domyślny.")));

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

    // Log z poprzedniego uruchomienia nie jest potrzebny.
    let _ = std::fs::remove_file(dir.join("akp03e.log"));
    state.log("info", format!("Start. Config: {}, panel: http://{}", config_path.display(), web::ADDR));

    // Panika w dowolnym wątku trafia do logu – bez konsoli inaczej ginie bez śladu.
    let panic_state = state.clone();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        panic_state.log("error", format!("Panika: {info}"));
        default_hook(info);
    }));

    system::watch_display(state.clone());
    actions::spawn_worker(state.clone(), actions_rx);

    // Tokio w osobnym wątku – główny wątek obsługuje ikonę w zasobniku.
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
            rt_state.log("info", "Zamykanie");
            let _ = rt_state.quit.send(true);
            // Czekamy tylko na posprzątanie kontrolera; panel z otwartym SSE nie zamknie się sam.
            let _ = tokio::time::timeout(Duration::from_secs(3), device_task).await;
            web_task.abort();
            // Odczyty HID działają w wątkach blokujących, na które runtime czekałby w nieskończoność.
            std::process::exit(0);
        });
    });

    if !from_autostart {
        // Daj serwerowi chwilę na start, zanim otworzymy przeglądarkę.
        let s = state.clone();
        std::thread::spawn(move || {
            for _ in 0..50 {
                if std::net::TcpStream::connect(web::ADDR).is_ok() {
                    system::open_panel();
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            s.log("error", "Panel nie wystartował w 10 s");
        });
    }

    tray::run(state);
}
