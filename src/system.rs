//! Windows integration: autostart (registry), single instance, display power, opening the panel.

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

/// If autostart was enabled for another path (e.g. after the program was moved),
/// rewrites the entry to point to the current .exe. Returns whether autostart is enabled.
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

/// Whether the registry has an autostart entry pointing to THIS .exe.
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
        Err(format!("registry error {}", status.0))
    }
}

/// Returns false if the program is already running. The mutex lives until the process ends.
pub fn acquire_single_instance() -> bool {
    unsafe {
        // The name must outlive GetLastError – dropping the HSTRING overwrites the thread's error code.
        let name = HSTRING::from("Local\\akp03e-controller");
        // The handle is deliberately never closed – Windows releases the mutex when the process ends.
        let _handle = CreateMutexW(None, true, &name);
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        drop(name);
        !already
    }
}

static DISPLAY_STATE: OnceLock<(Arc<AppState>, AtomicU32)> = OnceLock::new();

/// After waking up (including from Modern Standby) the controller can stop reporting key
/// presses even though drawing images still works. When the display comes back from "off",
/// we ask for a fresh connection to the controller.
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
        // 0 = off, 1 = on, 2 = dimmed.
        let value = unsafe { std::ptr::read_unaligned(setting.Data.as_ptr() as *const u32) };
        if let Some((state, last)) = DISPLAY_STATE.get() {
            let previous = last.swap(value, Ordering::Relaxed);
            if previous == 0 && value != 0 {
                state.log("info", "Display turned on after sleep – reconnecting to the controller");
                state.reconnect.notify_one();
            }
        }
        0
    }

    // The first callback (right after registering) reports the current state – assume "on".
    if DISPLAY_STATE.set((state.clone(), AtomicU32::new(1))).is_err() {
        return;
    }
    // The parameters must live as long as the registration, i.e. until the process ends.
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
        state.log("error", format!("Cannot watch the display state (error {})", status.0));
    }
}

pub fn open_panel() {
    let _ = std::process::Command::new("cmd")
        .raw_arg(format!("/C start \"\" \"http://{}\"", web::ADDR))
        .creation_flags(0x0800_0000)
        .spawn();
}
