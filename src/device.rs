//! Obsługa kontrolera: połączenie (z ponawianiem), rysowanie klawiszy, odczyt zdarzeń.
//! Protokół i mapowanie wejść za opendeck-akp03 (4ndv) / mirajazz.

use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, SystemTime},
};

use mirajazz::{
    device::{Device, DeviceQuery, list_devices},
    error::MirajazzError,
    state::DeviceStateUpdate,
    types::{DeviceInput, ImageFormat, ImageMirroring, ImageMode, ImageRotation},
};
use serde_json::json;

use crate::{
    config::{Action, ENCODERS, LCD_KEYS},
    render::render_key,
    state::AppState,
};

const VID: u16 = 0x0300;
const PID: u16 = 0x3002;
const QUERY: DeviceQuery = DeviceQuery::new(0xFFA0, 1, VID, PID);
const PROTOCOL_VERSION: usize = 3;
/// 6 klawiszy LCD + 3 przyciski bez ekranu.
const KEY_COUNT: usize = 9;

const IMAGE_FORMAT: ImageFormat = ImageFormat {
    mode: ImageMode::JPEG,
    size: (64, 64),
    rotation: ImageRotation::Rot90,
    mirror: ImageMirroring::None,
};

fn process_input(input: u8, state: u8) -> Result<DeviceInput, MirajazzError> {
    match input {
        0..=6 | 0x25 | 0x30 | 0x31 => {
            let mut buttons = vec![false; KEY_COUNT];
            let index = match input {
                0 => return Ok(DeviceInput::ButtonStateChange(buttons)),
                1..=6 => input as usize - 1,
                0x25 => 6,
                0x30 => 7,
                _ => 8,
            };
            buttons[index] = state != 0;
            Ok(DeviceInput::ButtonStateChange(buttons))
        }
        0x90 | 0x91 | 0x50 | 0x51 | 0x60 | 0x61 => {
            let mut twist = vec![0i8; ENCODERS];
            let (encoder, delta) = match input {
                0x90 => (0, -1),
                0x91 => (0, 1),
                0x50 => (1, -1),
                0x51 => (1, 1),
                0x60 => (2, -1),
                _ => (2, 1),
            };
            twist[encoder] = delta;
            Ok(DeviceInput::EncoderTwist(twist))
        }
        0x33..=0x35 => {
            let mut encoders = vec![false; ENCODERS];
            let encoder = match input {
                0x33 => 0,
                0x35 => 1,
                _ => 2,
            };
            encoders[encoder] = state != 0;
            Ok(DeviceInput::EncoderStateChange(encoders))
        }
        _ => Err(MirajazzError::BadData),
    }
}

pub async fn run(state: Arc<AppState>) {
    let mut quit = state.quit.subscribe();
    let mut announced_missing = false;
    loop {
        match session(&state, &mut quit, &mut announced_missing).await {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => state.log("error", format!("Kontroler: {e}")),
        }
        if state.connected.swap(false, Ordering::Relaxed) {
            state.log("info", "Kontroler rozłączony – czekam na ponowne podłączenie");
            state.emit_status();
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            _ = quit.changed() => return,
        }
    }
}

/// Zwraca Ok(true), gdy program się zamyka.
async fn session(
    state: &Arc<AppState>,
    quit: &mut tokio::sync::watch::Receiver<bool>,
    announced_missing: &mut bool,
) -> Result<bool, MirajazzError> {
    let Some(info) = list_devices(&[QUERY]).await?.into_iter().next() else {
        if !*announced_missing {
            state.log("info", "Nie widzę AKP03E – podłącz kontroler");
            *announced_missing = true;
        }
        return Ok(false);
    };
    *announced_missing = false;

    let device = Device::connect(&info, PROTOCOL_VERSION, KEY_COUNT, ENCODERS).await?;
    state.connected.store(true, Ordering::Relaxed);
    state.log("info", format!("Połączono z AKP03E (serial {})", device.serial_number()));
    state.emit_status();

    device.clear_all_button_images().await?;
    render_all(&device, state).await?;

    let reader = device.get_reader(process_input);
    let mut keep_alive = tokio::time::interval(Duration::from_secs(15));
    // Wykrywanie uśpienia bez pomocy systemu: proces zamrożony na czas uśpienia
    // widzi duży przeskok zegara ściennego między kolejnymi tyknięciami.
    let mut watchdog = tokio::time::interval(Duration::from_secs(5));
    let mut last_tick = SystemTime::now();

    loop {
        tokio::select! {
            _ = quit.changed() => {
                device.clear_all_button_images().await.ok();
                device.flush().await.ok();
                device.shutdown().await.ok();
                return Ok(true);
            }
            _ = state.reconnect.notified() => return Ok(false),
            _ = watchdog.tick() => {
                let now = SystemTime::now();
                let gap = now.duration_since(last_tick).unwrap_or_default();
                last_tick = now;
                if gap > Duration::from_secs(30) {
                    state.log("info", format!("Program był wstrzymany przez {} s (uśpienie?) – łączę ponownie z kontrolerem", gap.as_secs()));
                    return Ok(false);
                }
            }
            _ = keep_alive.tick() => device.keep_alive().await?,
            _ = state.rerender.notified() => render_all(&device, state).await?,
            updates = reader.read(Some(Duration::from_secs(1))) => {
                let updates = match updates {
                    Ok(u) => u,
                    Err(MirajazzError::BadData) => continue,
                    Err(e) => return Err(e),
                };
                for update in updates {
                    handle_update(state, update);
                }
            }
        }
    }
}

async fn render_all(device: &Device, state: &AppState) -> Result<(), MirajazzError> {
    if !state.screen_on.load(Ordering::Relaxed) {
        return device.set_brightness(0).await;
    }
    let (brightness, keys) = {
        let cfg = state.config.read().unwrap();
        let p = state.profile.load(Ordering::Relaxed).min(cfg.profiles.len() - 1);
        (cfg.brightness, cfg.profiles[p].keys.clone())
    };
    let mic_muted = state.mic_muted.load(Ordering::Relaxed);
    device.set_brightness(brightness).await?;
    for (i, key) in keys.iter().enumerate().take(LCD_KEYS) {
        let img = render_key(key, &state.icons_dir, mic_muted);
        device.set_button_image(i as u8, IMAGE_FORMAT, img).await?;
    }
    device.flush().await
}

fn handle_update(state: &AppState, update: DeviceStateUpdate) {
    let (event, action) = {
        let cfg = state.config.read().unwrap();
        let p = state.profile.load(Ordering::Relaxed).min(cfg.profiles.len() - 1);
        let key_target = |k: u8| {
            if (k as usize) < LCD_KEYS {
                ("key", k as usize, cfg.profiles[p].keys[k as usize].action.clone())
            } else {
                let b = k as usize - LCD_KEYS;
                ("button", b, cfg.buttons[b].clone())
            }
        };
        match update {
            DeviceStateUpdate::ButtonDown(k) => {
                let (target, index, action) = key_target(k);
                (json!({"type": "press", "target": target, "index": index, "down": true}), Some(action))
            }
            DeviceStateUpdate::ButtonUp(k) => {
                let (target, index, _) = key_target(k);
                (json!({"type": "press", "target": target, "index": index, "down": false}), None)
            }
            DeviceStateUpdate::EncoderDown(e) => (
                json!({"type": "press", "target": "knob", "index": e, "down": true}),
                Some(cfg.encoder(p, e as usize).press.clone()),
            ),
            DeviceStateUpdate::EncoderUp(e) => {
                (json!({"type": "press", "target": "knob", "index": e, "down": false}), None)
            }
            DeviceStateUpdate::EncoderTwist(e, delta) => {
                let enc = cfg.encoder(p, e as usize);
                let action = if delta > 0 { enc.right.clone() } else { enc.left.clone() };
                (json!({"type": "twist", "index": e, "delta": delta}), Some(action))
            }
        }
    };

    state.emit(event);
    if let Some(action) = action {
        execute(state, action);
    }
}

pub fn execute(state: &AppState, action: Action) {
    match action {
        Action::None => {}
        Action::ProfileNext => state.shift_profile(1),
        Action::ProfilePrev => state.shift_profile(-1),
        Action::ScreenToggle => {
            state.screen_on.fetch_xor(true, Ordering::Relaxed);
            state.rerender.notify_one();
            state.emit_status();
        }
        other => {
            let _ = state.actions.send(other);
        }
    }
}
