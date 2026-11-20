//! System tray icon. Runs on the main thread with a Win32 message loop.

use std::sync::{Arc, atomic::Ordering};

use tray_icon::{
    MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PostQuitMessage, SetTimer, TranslateMessage,
};

use crate::{state::AppState, system};

/// 32x32 icon: a dark rounded square with a 3x2 grid of coloured "keys".
fn icon_rgba() -> Vec<u8> {
    const S: i32 = 32;
    let colors = [
        [45, 106, 79], [59, 130, 246], [0, 101, 169],
        [0, 82, 204], [244, 104, 0], [124, 58, 237],
    ];
    let mut px = vec![0u8; (S * S * 4) as usize];
    for y in 0..S {
        for x in 0..S {
            // rounded corners (radius 6)
            let (cx, cy) = (x.clamp(6, S - 7), y.clamp(6, S - 7));
            let inside = (x - cx).pow(2) + (y - cy).pow(2) <= 36;
            if !inside {
                continue;
            }
            let mut c = [24u8, 27, 33];
            let (gx, gy) = ((x - 4) / 8, (y - 8) / 8);
            let (lx, ly) = ((x - 4) % 8, (y - 8) % 8);
            if x >= 4 && y >= 8 && gx < 3 && gy < 2 && lx < 7 && ly < 7 {
                c = colors[(gy * 3 + gx) as usize];
            }
            let i = ((y * S + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    px
}

pub fn run(state: Arc<AppState>) {
    let menu = Menu::new();
    let open = MenuItem::new("Open panel", true, None);
    let autostart = CheckMenuItem::new("Start with Windows", true, state.autostart.load(Ordering::Relaxed), None);
    let reconnect = MenuItem::new("Reconnect to controller", true, None);
    let quit = MenuItem::new("Quit", true, None);
    menu.append_items(&[&open, &reconnect, &autostart, &PredefinedMenuItem::separator(), &quit])
        .expect("menu");

    let icon = tray_icon::Icon::from_rgba(icon_rgba(), 32, 32).expect("icon");
    let _tray = TrayIconBuilder::new()
        .with_tooltip("AKP03E – click to open the panel")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .expect("tray icon");

    unsafe {
        // One-second timer: wakes the loop so the menu reflects changes made in the panel.
        SetTimer(None, 0, 1000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);

            let wanted = state.autostart.load(Ordering::Relaxed);
            if autostart.is_checked() != wanted {
                autostart.set_checked(wanted);
            }

            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    system::open_panel();
                }
            }

            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == *open.id() {
                    system::open_panel();
                } else if event.id == *reconnect.id() {
                    state.log("info", "Reconnecting to the controller (tray menu)");
                    state.reconnect.notify_one();
                } else if event.id == *autostart.id() {
                    state.set_autostart(autostart.is_checked());
                    autostart.set_checked(state.autostart.load(Ordering::Relaxed));
                } else if event.id == *quit.id() {
                    let _ = state.quit.send(true);
                    PostQuitMessage(0);
                }
            }
        }
    }
}
