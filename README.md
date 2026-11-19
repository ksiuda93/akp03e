# AKP03E

A small, self-contained Windows driver and configurator for the **Ajazz AKP03E** desk controller,
written in Rust.

The AKP03E is a compact macro pad in the Stream Deck family: **6 keys with 64×64 LCD screens**,
**3 plain buttons** and **3 rotary knobs** (each knob can also be pressed). This program talks to the
device directly over USB HID, draws the key images, listens for presses and knob turns, and turns them
into actions on your PC – launching apps, sending shortcuts, typing text, controlling audio and more.
Everything is configured from a web page served by the program itself.

## Why this exists

I bought the controller to have a physical "control surface" for everyday work: muting the microphone
during calls, adjusting volume per application, opening the tools I use all day (terminal, VS Code,
Jira, Grafana) and driving [Claude Code](https://claude.com/claude-code) without reaching for the
keyboard.

Instead of relying on the vendor's software, I wanted something I fully understand and control:

- **Own code, no black box** – one small executable, no account, no cloud, no telemetry. It only
  listens on `127.0.0.1`.
- **Built around my workflow** – actions that matter to a developer: per-app volume, a mic toggle
  that shows its real state on the key, typed commands with Enter, and a dedicated Claude Code profile.
- **Easy to change** – configuration lives in one readable TOML file and can be edited in a browser,
  by hand, or through a small HTTP API from scripts.
- **Open protocol** – the device protocol comes from the open-source
  [mirajazz](https://github.com/4ndv/mirajazz) library, so nothing here depends on vendor code.

## Features

- **Web panel** at `http://127.0.0.1:7878` – a picture of the device where you click a key, button or
  knob and edit it: label, background colour, icon (PNG/JPEG upload), and action. Every action has a
  **Test** button. Presses and knob turns on the real device are highlighted live.
- **Profiles** – each profile has its own six LCD keys. Buttons A/B/C are shared. Knobs are shared by
  default, but any profile can override any knob (e.g. the right knob becomes ↑/↓/Enter only in the
  Claude profile).
- **Actions**
  - open an application, file, folder or URL,
  - send a keyboard shortcut (`Ctrl+Shift+M`, `Win+Shift+S`, media keys, …),
  - type text, optionally followed by Enter,
  - media keys: play/pause, next, previous, stop,
  - system volume up/down and mute,
  - volume and mute of the **active application** (the window in focus),
  - microphone mute toggle – the key turns red with "OFF" while muted, even when muted from another app,
  - lock the workstation,
  - next/previous profile, turn the key screens off/on.
- **System tray icon** – open the panel, reconnect to the controller, toggle "start with Windows", quit.
- **Start with Windows** (per-user registry `Run` entry), single running instance.
- **Resilience** – the program reconnects automatically when the controller is unplugged and plugged
  back in, and after the PC wakes from sleep (including Modern Standby), when the device would
  otherwise stop reporting key presses.

The user interface and log messages are currently in Polish.

## Default layout

| Profile | LCD keys |
|---|---|
| Praca (Work) | Microphone toggle, Terminal (`wt.exe`), VS Code, Jira, Grafana, Screenshot (`Win+Shift+S`) |
| Media | Play/Pause, Previous, Next, Microphone toggle, Mute, Lock |
| Claude | Accept (`Enter`), Stop (`Esc`), Mode (`Shift+Tab`), `/clear`, `/compact`, New Claude (`wt.exe claude`) |

| Control | Action |
|---|---|
| Left knob | System volume ∓2 %, press = mute |
| Top knob | Active application volume ∓5 %, press = mute that application |
| Right knob | Previous / next track, press = play/pause (Claude profile: `↑` / `↓`, press = `Enter`) |
| Button A / B | Previous / next profile |
| Button C | Turn the key screens off/on |

The Jira and Grafana URLs are placeholders – change them in the panel.

### The Claude Code profile

Shortcuts and text go to the focused window, so keep the terminal with Claude Code in front:

- **Accept** confirms a permission prompt or sends the message, **Stop** interrupts Claude,
  **Mode** cycles the permission mode (default → accept edits → plan).
- **/clear** and **/compact** type the command and press Enter.
- **New Claude** opens Windows Terminal with a fresh `claude` session.
- The **right knob** moves through selection menus (permission prompts, `/model`, …) and its press
  confirms the choice.

## Getting started

### Requirements

- Windows 10/11 and an Ajazz AKP03E (revision 2, USB ID `0300:3002`).
- [Rust](https://rustup.rs/). The repository pins the `stable-x86_64-pc-windows-gnu` toolchain in
  `rust-toolchain.toml`, so the MSVC build tools are not needed.
- MinGW-w64 on `PATH`, for example:

  ```powershell
  winget install --source winget BrechtSanders.WinLibs.POSIX.UCRT
  ```

### Install

```powershell
git clone https://github.com/ksiuda93/akp03e.git
cd akp03e
.\install.ps1
```

`install.ps1` builds the release version, copies it to
`%LOCALAPPDATA%\Programs\akp03e\akp03e-panel.exe`, stops the old instance if it is running, and starts
the new one. The panel opens in your browser; afterwards use the tray icon. Enable
"Uruchamiaj z Windows" (start with Windows) in the panel or the tray menu.

### Develop

```powershell
cargo run
```

The debug build keeps a console window with the log. The release build runs in the background and
writes its log to a file instead.

### Files

| Path | Contents |
|---|---|
| `%APPDATA%\akp03e\config.toml` | configuration (created with the defaults on first start) |
| `%APPDATA%\akp03e\icons\` | uploaded key icons |
| `%APPDATA%\akp03e\akp03e.log` | log of the current run |

Helper scripts:

- `status.ps1` – running processes, the autostart entry, the end of the log and the panel state,
- `reconnect.ps1` – asks the running program to reconnect to the controller.

## Documentation

- [Configuration and actions](docs/configuration.md) – `config.toml` format, every action type,
  key names for shortcuts, the HTTP API.
- [Architecture](docs/architecture.md) – how the program is put together, the device protocol,
  threads, and the design decisions behind them.

## Troubleshooting

- **The controller shows images but ignores presses** – use "Połącz ponownie z kontrolerem"
  (reconnect) in the tray menu. This normally happens on its own after the PC wakes up.
- **The program disappears or settings are not saved** – the executable is unsigned and sends
  keystrokes, which some antivirus products treat as suspicious. Norton, for example, quarantined it
  and ran a freshly built copy in isolation, so its file writes never reached the disk. Adding an
  exclusion for `%LOCALAPPDATA%\Programs\akp03e` solved it.
- **A shortcut or text does nothing in one window** – Windows does not let a normal program send
  input to a window running as administrator.
- **"Port 7878 in use"** – another copy is still shutting down; the program retries for a few seconds.

## Credits

- [mirajazz](https://github.com/4ndv/mirajazz) – the device protocol library for Mirabox/Ajazz devices.
- [opendeck-akp03](https://github.com/4ndv/opendeck-akp03) – the reference for the AKP03 input mapping.
