# Configuration

The configuration lives in `%APPDATA%\akp03e\config.toml`. It is created with the default profiles
on the first start. The web panel edits this file for you; you can also edit it by hand while the
program is stopped. If the file has a syntax error, the program shows a message and exits instead of
overwriting it – fix it or delete it to get the defaults back.

## Structure

```toml
brightness = 70            # key screen brightness, 0–100

# Buttons A, B, C (shared by all profiles), in this order
[[buttons]]
type = "profile_prev"
[[buttons]]
type = "profile_next"
[[buttons]]
type = "screen_toggle"

# Knobs: left, top, right (shared by all profiles unless a profile overrides them)
[[encoders]]
left  = { type = "system_volume", step = -2 }
right = { type = "system_volume", step = 2 }
press = { type = "system_mute" }
# … two more [[encoders]] blocks

[[profiles]]
name = "Claude"

# Six LCD keys per profile, left to right, top row first
[[profiles.keys]]
label = "Accept"           # text on the key; "\n" starts a new line
color = "#2d6a4f"          # background colour
icon = "claude.png"        # optional, a file in %APPDATA%\akp03e\icons
action = { type = "hotkey", keys = "Enter" }
# … five more keys

# Optional: knobs that behave differently in this profile.
# The key is the knob number: "0" = left, "1" = top, "2" = right.
[profiles.encoders.2]
left  = { type = "hotkey", keys = "Up" }
right = { type = "hotkey", keys = "Down" }
press = { type = "hotkey", keys = "Enter" }
```

Missing keys, buttons and knobs are filled with "no action", so a short file is still valid.

### How a key is drawn

Each LCD key is rendered as a 64×64 image:

- the background colour fills the key,
- the icon, if any, is drawn in the middle (56 px on its own, 36 px above the label),
- the label is centred, shrinks automatically to fit, and is black or white depending on how light
  the background is,
- a key with the `mic_toggle` action turns red and shows "OFF" while the microphone is muted.

## Actions

Every action has a `type`. Some take extra fields.

| `type` | Fields | What it does |
|---|---|---|
| `none` | – | Nothing. |
| `open` | `target`, `args` (optional) | Opens an app, file, folder or URL, like `start` in `cmd`: `wt.exe`, `code`, `https://…`, `C:\Path\app.exe`, `ms-settings:`. |
| `hotkey` | `keys` | Presses a key combination, e.g. `Ctrl+Shift+M`. See [key names](#key-names). |
| `text` | `text`, `enter` (optional, default `false`) | Types the text into the focused window; with `enter = true` it presses Enter afterwards. |
| `media` | `key`: `play_pause`, `next`, `prev`, `stop` | Media key. |
| `system_volume` | `step` | Changes the default output volume by `step` percentage points (negative = quieter). Turning it up also unmutes. |
| `system_mute` | – | Toggles mute of the default output device. |
| `app_volume` | `step` | Changes the volume of the application whose window is in focus. |
| `app_mute` | – | Toggles mute of the application in focus. |
| `mic_toggle` | – | Toggles mute of the default microphone. |
| `lock` | – | Locks the workstation. |
| `profile_next` / `profile_prev` | – | Switches to the next / previous profile (wraps around). |
| `screen_toggle` | – | Turns the key screens off and on. |

Notes:

- `hotkey` and `text` go to the window in focus. Windows blocks input to windows running as
  administrator.
- `app_volume` and `app_mute` match audio sessions by the executable name of the focused window, so
  they also work for browsers, which play sound from child processes. If the application is not
  playing anything, the log says so.
- The microphone state is checked every 2 seconds, so the key also follows mutes made in other apps.

### Key names

Combine keys with `+`. Names are case-insensitive.

| Group | Names |
|---|---|
| Modifiers | `Ctrl`, `Shift`, `Alt`, `Win` |
| Letters and digits | `A`–`Z`, `0`–`9` |
| Function keys | `F1`–`F24` |
| Editing | `Enter`, `Esc`, `Tab`, `Space`, `Backspace`, `Delete`, `Insert` |
| Navigation | `Left`, `Up`, `Right`, `Down`, `Home`, `End`, `PageUp`, `PageDown` |
| Other | `PrintScreen` |
| Media | `PlayPause`, `NextTrack`, `PrevTrack`, `StopMedia`, `VolumeUp`, `VolumeDown`, `VolumeMute` |
| Punctuation | `-`, `=` (or `plus`), `,`, `.`, `/`, `;`, `` ` ``, `[`, `]`, `\`, `'` |

The panel has a **Record** button that fills the field by pressing the combination.

## HTTP API

The panel uses a small JSON API, which you can also call from scripts. It listens only on
`127.0.0.1:7878`.

| Method and path | Description |
|---|---|
| `GET /api/config` | The whole configuration as JSON (same structure as the TOML file). |
| `PUT /api/config` | Replaces and saves the configuration. Other open panels reload it automatically. |
| `GET /api/state` | `{ connected, profile, screen_on, mic_muted, autostart }`. |
| `POST /api/profile/{index}` | Switches to a profile (0-based). |
| `POST /api/test` | Runs the action in the JSON body, e.g. `{"type":"media","key":"play_pause"}`. |
| `GET /api/preview?profile=P&key=K` | PNG preview of a key. |
| `POST /api/icons?name=file.png` | Uploads an icon (raw PNG/JPEG body); returns `{ "name": … }` to use as `icon`. |
| `POST /api/autostart/{true\|false}` | Enables or disables starting with Windows. |
| `POST /api/reconnect` | Reconnects to the controller. |
| `GET /api/events` | Server-sent events: `status`, `press`, `twist`, `log` and `config` messages. |

PowerShell example – switch to the second profile and play/pause:

```powershell
Invoke-RestMethod -Method Post http://127.0.0.1:7878/api/profile/1
Invoke-RestMethod -Method Post http://127.0.0.1:7878/api/test -ContentType 'application/json' -Body '{"type":"media","key":"play_pause"}'
```
