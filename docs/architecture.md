# Architecture

One executable, no installer, no services. It runs in the user session, because it needs that
session to send keystrokes, read the focused window and control audio.

```
            ┌──────────────── akp03e-panel.exe ────────────────┐
 USB HID ◄──┤ device task ──► actions worker ──► Win32 / Core Audio
 (AKP03E)   │      ▲  │             ▲                          │
            │      │  └── events ───┼──► web server ◄──────────┼──► browser panel
            │  AppState (config, profile, flags, channels)     │    http://127.0.0.1:7878
            │      ▲                                           │
            │  tray icon (main thread) · display-power callback│
            └──────────────────────────────────────────────────┘
```

## Source layout

| File | Responsibility |
|---|---|
| `src/main.rs` | Start-up: single-instance check, data folder, config, panic logging, spawning everything. |
| `src/state.rs` | `AppState` shared by all parts: configuration, active profile, flags, channels, logging. |
| `src/config.rs` | Configuration types, defaults (including the Claude profile), loading and saving TOML. |
| `src/device.rs` | Talking to the controller: connect/reconnect, drawing keys, reading input, mapping it to actions. |
| `src/render.rs` | Drawing a 64×64 key image from colour, icon and label. |
| `src/actions.rs` | Running actions in Windows: launching, `SendInput`, Core Audio, lock. |
| `src/web.rs` | HTTP API and server-sent events (axum). |
| `src/panel.html` | The whole web panel (plain HTML/CSS/JS, compiled into the executable). |
| `src/tray.rs` | Tray icon and its menu, the Win32 message loop. |
| `src/system.rs` | Windows integration: autostart registry entry, single-instance mutex, display power notifications, opening the panel. |

## Threads

- **Main thread** – the tray icon. Win32 requires the icon's window and message loop to live on one
  thread; a 1-second timer wakes the loop so the menu reflects changes made in the panel.
- **Tokio runtime thread** – two tasks: the device session and the web server.
- **Actions worker** – one dedicated thread with COM initialised. Core Audio is COM-based, and a
  single thread keeps actions ordered and away from the async runtime. Every 2 seconds it also polls
  the microphone mute state.
- **Display-power callback** – Windows calls it on a thread-pool thread when the screen turns on or off.

The parts talk through `AppState`: a `RwLock` for the configuration, atomics for flags
(profile, screen on, microphone muted, connected, autostart), `Notify` handles for "redraw the keys"
and "reconnect", an `mpsc` channel for actions and a `broadcast` channel for events sent to panels.

## Device protocol

The controller is a Mirabox-based device; the [mirajazz](https://github.com/4ndv/mirajazz) crate
implements its protocol, and the input mapping follows
[opendeck-akp03](https://github.com/4ndv/opendeck-akp03).

- USB `0300:3002`, the vendor HID interface (usage page `0xFFA0`, usage `1`). The second interface is
  a plain keyboard and is not used.
- Protocol version 3, 9 "buttons" (6 LCD keys + 3 plain buttons) and 3 encoders.
- Key images are JPEG, 64×64, rotated by 90°.
- The device expects a keep-alive message; the program sends one every 15 seconds.
- Input reports carry a code and a pressed/released state:

| Code | Meaning |
|---|---|
| `0x01`–`0x06` | LCD keys 1–6 |
| `0x25`, `0x30`, `0x31` | buttons A, B, C |
| `0x90` / `0x91` | left knob left / right |
| `0x50` / `0x51` | top knob left / right |
| `0x60` / `0x61` | right knob left / right |
| `0x33`, `0x35`, `0x34` | left, top, right knob press |

## Device session lifecycle

`device::run` loops forever: find the device, run a session, and if the session ends for any reason,
wait 2 seconds and try again. A session:

1. connects, clears the keys and draws the active profile,
2. waits in a `select!` on: quit, keep-alive timer, redraw requests, reconnect requests, a 5-second
   watchdog, and device input (read with a 1-second timeout).

The session ends, and is therefore re-created, when:

- the device returns an error (unplugged),
- **the display turns back on** after being off – reported by Windows through
  `PowerSettingRegisterNotification(GUID_CONSOLE_DISPLAY_STATE)`,
- **the watchdog sees a wall-clock jump of more than 30 seconds**, meaning the process was frozen
  (sleep or Modern Standby),
- the user picks "reconnect" in the tray or calls `POST /api/reconnect`.

The reconnect logic exists because of a real failure: after the PC spent a night in Modern Standby,
the controller kept accepting images but no key presses arrived any more. Opening a fresh connection
fixes it, so the program now does that automatically on every wake-up.

## Web panel

The panel is a single HTML file compiled into the binary with `include_str!`, so there is nothing to
deploy. It loads the configuration through the API, edits it in memory and saves it with a 400 ms
debounce. Server-sent events keep it live: key presses are highlighted, knobs spin, the log scrolls,
and the status follows the device.

Two details prevent lost changes:

- The panel never replaces its configuration object with the server's reply – the open editor holds
  references into that object, and replacing it would make later edits go nowhere.
- Each panel sends its own client ID with `PUT /api/config`. The server broadcasts a `config` event;
  every *other* panel reloads, so an old browser tab cannot overwrite changes made elsewhere. Panels
  also reload after the event stream reconnects (the program may have restarted).

## Windows integration

- **Single instance** – a named mutex `Local\akp03e-controller`. A second start opens the panel in
  the browser and exits.
- **Autostart** – `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\AKP03E` pointing to the
  executable with `--autostart` (which skips opening the browser). If the executable moves, the entry
  is updated on the next start.
- **Background mode** – release builds use the `windows` subsystem (no console) and log to
  `%APPDATA%\akp03e\akp03e.log`; panics are written there too.
- **Shutdown** – the device is cleared and shut down, then the process exits immediately; waiting
  for blocking HID reads and open event streams would otherwise take a long time.

## Build notes

The project builds with the GNU toolchain (`stable-x86_64-pc-windows-gnu`) and MinGW-w64. The
`windows` crate needs `dlltool` from MinGW for its import libraries. This avoids a dependency on
Visual Studio and the Windows SDK, which were broken on the machine this was written on.
