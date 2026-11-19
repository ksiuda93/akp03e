//! Integracja z Windows: autostart (rejestr), jedna instancja, otwieranie panelu.

use std::{
    ffi::c_void,
    os::windows::process::CommandExt,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};

use windows::{
    Win32::{
        Foundation::{ERROR_ALREADY_EXISTS, ERROR_SUCCESS, GetLastError, HANDLE},
        System::{
            Power::{
                DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, POWERBROADCAST_SETTING,
                PowerSettingRegisterNotification,
            },
            SystemServices::GUID_CONSOLE_DISPLAY_STATE,
            Registry::{
                HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
                RegSetKeyValueW,
            },
            Threading::CreateMutexW,
        },
        UI::WindowsAndMessaging::DEVICE_NOTIFY_CALLBACK,
    },
    core::{HSTRING, w},
};

use crate::{state::AppState, web};

const RUN_KEY: windows::core::PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const RUN_VALUE: windows::core::PCWSTR = w!("AKP03E");
pub const AUTOSTART_ARG: &str = "--autostart";

fn autostart_command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(format!("\"{}\" {AUTOSTART_ARG}", exe.display()))
}

/// Jeśli autostart był włączony dla innej ścieżki (np. po przeniesieniu programu),
/// przepisuje wpis na bieżący plik .exe. Zwraca, czy autostart jest włączony.
pub fn refresh_autostart() -> bool {
    if autostart_enabled() {
        return true;
    }
    if read_run_value().is_some() {
        return set_autostart(true).is_ok();
    }
    false
}

fn read_run_value() -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut len),
        )
    };
    (status == ERROR_SUCCESS).then(|| String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]))
}

/// Czy w rejestrze jest wpis autostartu wskazujący na TEN plik .exe.
pub fn autostart_enabled() -> bool {
    let Some(value) = read_run_value() else { return false };
    autostart_command().map(|cmd| cmd.eq_ignore_ascii_case(&value)).unwrap_or(false)
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let status = unsafe {
        if enabled {
            let cmd: Vec<u16> = autostart_command()?.encode_utf16().chain([0]).collect();
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ.0,
                Some(cmd.as_ptr() as *const _),
                (cmd.len() * 2) as u32,
            )
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE)
        }
    };
    if status == ERROR_SUCCESS || (!enabled && status.0 == 2) {
        Ok(())
    } else {
        Err(format!("rejestr: błąd {}", status.0))
    }
}

/// Zwraca false, jeśli program już działa. Mutex żyje do końca procesu.
pub fn acquire_single_instance() -> bool {
    unsafe {
        // Nazwa musi żyć do GetLastError – zwolnienie HSTRING nadpisuje kod błędu wątku.
        let name = HSTRING::from("Local\\akp03e-controller");
        // Uchwyt celowo nie jest zamykany – system zwolni mutex przy końcu procesu.
        let _handle = CreateMutexW(None, true, &name);
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        drop(name);
        !already
    }
}

static DISPLAY_STATE: OnceLock<(Arc<AppState>, AtomicU32)> = OnceLock::new();

/// Po wybudzeniu (także z Modern Standby) odczyt naciśnięć z kontrolera potrafi przestać
/// działać, choć zapis obrazów nadal działa. Gdy ekran wraca ze stanu „wyłączony”,
/// prosimy o ponowne połączenie z kontrolerem.
pub fn watch_display(state: Arc<AppState>) {
    unsafe extern "system" fn callback(_: *const c_void, kind: u32, setting: *const c_void) -> u32 {
        const PBT_POWERSETTINGCHANGE: u32 = 0x8013;
        if kind != PBT_POWERSETTINGCHANGE || setting.is_null() {
            return 0;
        }
        let setting = unsafe { &*(setting as *const POWERBROADCAST_SETTING) };
        if setting.PowerSetting != GUID_CONSOLE_DISPLAY_STATE || setting.DataLength < 4 {
            return 0;
        }
        // 0 = wyłączony, 1 = włączony, 2 = przyciemniony.
        let value = unsafe { std::ptr::read_unaligned(setting.Data.as_ptr() as *const u32) };
        if let Some((state, last)) = DISPLAY_STATE.get() {
            let previous = last.swap(value, Ordering::Relaxed);
            if previous == 0 && value != 0 {
                state.log("info", "Ekran włączony po uśpieniu – łączę ponownie z kontrolerem");
                state.reconnect.notify_one();
            }
        }
        0
    }

    // Pierwsze wywołanie (zaraz po rejestracji) podaje bieżący stan – zakładamy „włączony”.
    if DISPLAY_STATE.set((state.clone(), AtomicU32::new(1))).is_err() {
        return;
    }
    // Parametry muszą żyć tak długo jak rejestracja, czyli do końca procesu.
    let params: &'static mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS =
        Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS { Callback: Some(callback), Context: std::ptr::null_mut() }));
    let mut handle: *mut c_void = std::ptr::null_mut();
    let status = unsafe {
        PowerSettingRegisterNotification(
            &GUID_CONSOLE_DISPLAY_STATE,
            DEVICE_NOTIFY_CALLBACK,
            HANDLE(params as *mut _ as *mut c_void),
            &mut handle,
        )
    };
    if status != ERROR_SUCCESS {
        state.log("error", format!("Nie można śledzić stanu ekranu (błąd {})", status.0));
    }
}

pub fn open_panel() {
    let _ = std::process::Command::new("cmd")
        .raw_arg(format!("/C start \"\" \"http://{}\"", web::ADDR))
        .creation_flags(0x0800_0000)
        .spawn();
}
