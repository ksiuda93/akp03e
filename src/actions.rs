//! Wykonywanie akcji w Windows: dźwięk (Core Audio), klawisze (SendInput), uruchamianie.
//!
//! Wszystko działa w jednym dedykowanym wątku z zainicjalizowanym COM.

use std::{
    os::windows::process::CommandExt,
    process::Command,
    sync::{Arc, atomic::Ordering, mpsc},
    time::Duration,
};

use windows::{
    Win32::{
        Foundation::{BOOL, CloseHandle},
        Media::Audio::{
            Endpoints::IAudioEndpointVolume, IAudioSessionControl2, IAudioSessionManager2,
            IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, eCapture, eConsole,
            eRender, EDataFlow,
        },
        System::{
            Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
            Shutdown::LockWorkStation,
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::{
            Input::KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
                KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY,
            },
            WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        },
    },
    core::{PWSTR, Result as WinResult},
};

use crate::{
    config::{Action, MediaKey},
    state::AppState,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn spawn_worker(state: Arc<AppState>, rx: mpsc::Receiver<Action>) {
    std::thread::Builder::new()
        .name("akcje".into())
        .spawn(move || {
            unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }.expect("COM init");
            poll_mic(&state);
            loop {
                match rx.recv_timeout(Duration::from_secs(2)) {
                    Ok(action) => {
                        if let Err(e) = run(&state, &action) {
                            state.log("error", format!("{action:?}: {e}"));
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                poll_mic(&state);
            }
        })
        .expect("wątek akcji");
}

/// Sprawdza, czy mikrofon jest wyciszony (też przez inne aplikacje) i odświeża ikonę.
fn poll_mic(state: &AppState) {
    let Ok(muted) = endpoint_volume(eCapture).and_then(|v| unsafe { v.GetMute() }) else {
        return;
    };
    let muted = muted.as_bool();
    if state.mic_muted.swap(muted, Ordering::Relaxed) != muted {
        state.rerender.notify_one();
        state.emit_status();
    }
}

fn run(state: &AppState, action: &Action) -> Result<(), String> {
    match action {
        Action::Open { target, args } => open(target, args),
        Action::Hotkey { keys } => send_hotkey(keys),
        Action::Text { text, enter } => {
            send_text(text)?;
            if *enter {
                // Chwila przerwy, żeby aplikacja przetworzyła wpisany tekst przed Enterem.
                std::thread::sleep(Duration::from_millis(60));
                send_hotkey("Enter")?;
            }
            Ok(())
        }
        Action::Media { key } => send_hotkey(match key {
            MediaKey::PlayPause => "PlayPause",
            MediaKey::Next => "NextTrack",
            MediaKey::Prev => "PrevTrack",
            MediaKey::Stop => "StopMedia",
        }),
        Action::SystemVolume { step } => system_volume(*step).map_err(|e| e.to_string()),
        Action::SystemMute => toggle_endpoint_mute(eRender).map(|_| ()).map_err(|e| e.to_string()),
        Action::MicToggle => {
            let muted = toggle_endpoint_mute(eCapture).map_err(|e| e.to_string())?;
            state.mic_muted.store(muted, Ordering::Relaxed);
            state.rerender.notify_one();
            state.emit_status();
            state.log("info", if muted { "Mikrofon wyciszony" } else { "Mikrofon włączony" });
            Ok(())
        }
        Action::AppVolume { step } => app_volume(state, Some(*step)),
        Action::AppMute => app_volume(state, None),
        Action::Lock => unsafe { LockWorkStation() }.map_err(|e| e.to_string()),
        // Obsługiwane w zadaniu urządzenia.
        Action::None | Action::ProfileNext | Action::ProfilePrev | Action::ScreenToggle => Ok(()),
    }
}

// ---------- uruchamianie ----------

fn open(target: &str, args: &str) -> Result<(), String> {
    if target.trim().is_empty() {
        return Err("pusty cel".into());
    }
    // `start` rozwiązuje PATH, skróty .cmd (np. `code`), pliki, foldery i URL-e.
    // Cudzysłowy chronią znaki specjalne cmd (np. & w URL-ach).
    Command::new("cmd")
        .raw_arg(format!("/C start \"\" \"{}\" {}", target.trim(), args))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ---------- klawiatura ----------

fn key_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<(), String> {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err("SendInput odrzucił część zdarzeń (np. aktywne okno działa jako administrator)".into())
    }
}

/// Zwraca (kod VK, czy klawisz rozszerzony).
fn parse_key(name: &str) -> Option<(u16, bool)> {
    let n = name.trim().to_ascii_lowercase();
    let vk = match n.as_str() {
        "ctrl" | "control" => (0x11, false),
        "shift" => (0x10, false),
        "alt" => (0x12, false),
        "win" | "super" | "meta" | "cmd" => (0x5B, true),
        "enter" | "return" => (0x0D, false),
        "esc" | "escape" => (0x1B, false),
        "tab" => (0x09, false),
        "space" | "spacja" => (0x20, false),
        "backspace" => (0x08, false),
        "delete" | "del" => (0x2E, true),
        "insert" | "ins" => (0x2D, true),
        "home" => (0x24, true),
        "end" => (0x23, true),
        "pageup" | "pgup" => (0x21, true),
        "pagedown" | "pgdn" => (0x22, true),
        "left" => (0x25, true),
        "up" => (0x26, true),
        "right" => (0x27, true),
        "down" => (0x28, true),
        "printscreen" | "prtsc" => (0x2C, true),
        "playpause" => (0xB3, true),
        "nexttrack" => (0xB0, true),
        "prevtrack" => (0xB1, true),
        "stopmedia" => (0xB2, true),
        "volumeup" => (0xAF, true),
        "volumedown" => (0xAE, true),
        "volumemute" => (0xAD, true),
        "-" | "minus" => (0xBD, false),
        "=" | "plus" => (0xBB, false),
        "," => (0xBC, false),
        "." => (0xBE, false),
        "/" => (0xBF, false),
        ";" => (0xBA, false),
        "`" => (0xC0, false),
        "[" => (0xDB, false),
        "]" => (0xDD, false),
        "\\" => (0xDC, false),
        "'" => (0xDE, false),
        s if s.len() == 1 && s.as_bytes()[0].is_ascii_alphanumeric() => {
            (s.as_bytes()[0].to_ascii_uppercase() as u16, false)
        }
        s if s.starts_with('f') && s.len() <= 3 => {
            let n: u16 = s[1..].parse().ok()?;
            if !(1..=24).contains(&n) {
                return None;
            }
            (0x70 + n - 1, false)
        }
        _ => return None,
    };
    Some(vk)
}

fn send_hotkey(combo: &str) -> Result<(), String> {
    let keys = combo
        .split('+')
        .filter(|p| !p.trim().is_empty())
        .map(|p| parse_key(p).ok_or_else(|| format!("nieznany klawisz \"{}\"", p.trim())))
        .collect::<Result<Vec<_>, _>>()?;
    if keys.is_empty() {
        return Err("pusty skrót".into());
    }
    let flags = |ext: bool, up: bool| {
        let mut f = KEYBD_EVENT_FLAGS(0);
        if ext {
            f |= KEYEVENTF_EXTENDEDKEY;
        }
        if up {
            f |= KEYEVENTF_KEYUP;
        }
        f
    };
    let mut inputs: Vec<INPUT> = keys.iter().map(|&(vk, ext)| key_input(vk, 0, flags(ext, false))).collect();
    inputs.extend(keys.iter().rev().map(|&(vk, ext)| key_input(vk, 0, flags(ext, true))));
    send(&inputs)
}

fn send_text(text: &str) -> Result<(), String> {
    let mut inputs = vec![];
    for unit in text.encode_utf16() {
        inputs.push(key_input(0, unit, KEYEVENTF_UNICODE));
        inputs.push(key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }
    send(&inputs)
}

// ---------- dźwięk ----------

fn endpoint_volume(flow: EDataFlow) -> WinResult<IAudioEndpointVolume> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(flow, eConsole)?;
        device.Activate(CLSCTX_ALL, None)
    }
}

fn system_volume(step: i32) -> WinResult<()> {
    unsafe {
        let vol = endpoint_volume(eRender)?;
        let current = vol.GetMasterVolumeLevelScalar()?;
        let next = (current + step as f32 / 100.0).clamp(0.0, 1.0);
        vol.SetMasterVolumeLevelScalar(next, std::ptr::null())?;
        if step > 0 {
            vol.SetMute(BOOL::from(false), std::ptr::null())?;
        }
        Ok(())
    }
}

/// Przełącza wyciszenie i zwraca nowy stan.
fn toggle_endpoint_mute(flow: EDataFlow) -> WinResult<bool> {
    unsafe {
        let vol = endpoint_volume(flow)?;
        let muted = !vol.GetMute()?.as_bool();
        vol.SetMute(BOOL::from(muted), std::ptr::null())?;
        Ok(muted)
    }
}

fn process_name(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        Some(path.rsplit('\\').next()?.to_ascii_lowercase())
    }
}

/// Zmienia głośność (Some(step)) lub przełącza wyciszenie (None) aktywnej aplikacji.
/// Dopasowanie po nazwie .exe, bo np. przeglądarki grają dźwięk z procesów potomnych.
fn app_volume(state: &AppState, step: Option<i32>) -> Result<(), String> {
    let pid = unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid
    };
    let exe = process_name(pid).ok_or("nie można ustalić aktywnej aplikacji")?;

    fn ctx<T>(what: &str, r: WinResult<T>) -> Result<T, String> {
        r.map_err(|e| format!("{what}: {e}"))
    }

    let result: Result<usize, String> = unsafe {
        (|| {
            let enumerator: IMMDeviceEnumerator =
                ctx("MMDeviceEnumerator", CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL))?;
            let device = ctx("GetDefaultAudioEndpoint", enumerator.GetDefaultAudioEndpoint(eRender, eConsole))?;
            let manager: IAudioSessionManager2 = ctx("Activate(SessionManager2)", device.Activate(CLSCTX_ALL, None))?;
            let sessions = ctx("GetSessionEnumerator", manager.GetSessionEnumerator())?;
            let mut matched = 0;
            let mut target_mute: Option<bool> = None;
            for i in 0..ctx("GetCount", sessions.GetCount())? {
                let Ok(control) = sessions.GetSession(i) else { continue };
                let Ok(control2) = windows::core::Interface::cast::<IAudioSessionControl2>(&control) else { continue };
                // GetProcessId zwraca błąd dla sesji wieloprocesowych – pomijamy je.
                let spid = control2.GetProcessId().unwrap_or(0);
                if spid == 0 || process_name(spid).as_deref() != Some(exe.as_str()) {
                    continue;
                }
                let volume: ISimpleAudioVolume = ctx("ISimpleAudioVolume", windows::core::Interface::cast(&control))?;
                match step {
                    Some(step) => {
                        let v = (ctx("GetMasterVolume", volume.GetMasterVolume())? + step as f32 / 100.0).clamp(0.0, 1.0);
                        ctx("SetMasterVolume", volume.SetMasterVolume(v, std::ptr::null()))?;
                    }
                    None => {
                        // Wszystkie sesje aplikacji dostają ten sam stan.
                        let mute = *target_mute.get_or_insert(!ctx("GetMute", volume.GetMute())?.as_bool());
                        ctx("SetMute", volume.SetMute(BOOL::from(mute), std::ptr::null()))?;
                    }
                }
                matched += 1;
            }
            Ok(matched)
        })()
    };

    match result {
        Ok(0) => {
            state.log("info", format!("{exe}: aplikacja nie odtwarza teraz dźwięku"));
            Ok(())
        }
        Ok(_) => Ok(()),
        Err(e) => Err(format!("{exe}: {e}")),
    }
}
