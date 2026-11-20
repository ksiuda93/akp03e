//! Controller configuration stored in `config.toml`.

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

pub const LCD_KEYS: usize = 6;
pub const SIDE_BUTTONS: usize = 3;
pub const ENCODERS: usize = 3;

/// Action assigned to a key, button or knob.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    #[default]
    None,
    /// Opens an application, file, folder or URL (like `start` in cmd).
    Open {
        target: String,
        #[serde(default)]
        args: String,
    },
    /// Keyboard shortcut, e.g. "Ctrl+Shift+M".
    Hotkey { keys: String },
    /// Types text; `enter` also confirms it (e.g. a Claude Code command).
    Text {
        text: String,
        #[serde(default)]
        enter: bool,
    },
    Media { key: MediaKey },
    /// Changes the system volume by `step` percentage points.
    SystemVolume { step: i32 },
    SystemMute,
    /// Changes the volume of the application whose window is in focus.
    AppVolume { step: i32 },
    AppMute,
    MicToggle,
    Lock,
    ProfileNext,
    ProfilePrev,
    ScreenToggle,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKey {
    PlayPause,
    Next,
    Prev,
    Stop,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct KeyConfig {
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub action: Action,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EncoderConfig {
    #[serde(default)]
    pub left: Action,
    #[serde(default)]
    pub right: Action,
    #[serde(default)]
    pub press: Action,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub keys: Vec<KeyConfig>,
    /// Knobs that differ from the shared setup – the key is the knob number ("0" left,
    /// "1" top, "2" right). No entry = the shared setup applies.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub encoders: BTreeMap<String, EncoderConfig>,
}

impl Config {
    /// The knob setup that applies in the given profile.
    pub fn encoder(&self, profile: usize, index: usize) -> &EncoderConfig {
        self.profiles
            .get(profile)
            .and_then(|p| p.encoders.get(&index.to_string()))
            .unwrap_or(&self.encoders[index])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    /// Buttons without a screen (A, B, C) – shared by all profiles.
    #[serde(default)]
    pub buttons: Vec<Action>,
    /// Knobs: left, top, right – shared by all profiles unless a profile overrides them.
    #[serde(default)]
    pub encoders: Vec<EncoderConfig>,
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

fn default_color() -> String {
    "#1f2430".into()
}

fn default_brightness() -> u8 {
    70
}

fn key(label: &str, color: &str, action: Action) -> KeyConfig {
    KeyConfig {
        label: label.into(),
        color: color.into(),
        icon: None,
        action,
    }
}

fn open(target: &str) -> Action {
    Action::Open {
        target: target.into(),
        args: String::new(),
    }
}

/// Controls Claude Code in a terminal (acts on the focused window).
pub fn claude_profile() -> Profile {
    let command = |text: &str| Action::Text { text: text.into(), enter: true };
    let hotkey = |keys: &str| Action::Hotkey { keys: keys.into() };
    Profile {
        name: "Claude".into(),
        keys: vec![
            key("Accept", "#2d6a4f", hotkey("Enter")),
            key("Stop", "#b42318", hotkey("Esc")),
            key("Mode", "#7c3aed", hotkey("Shift+Tab")),
            key("/clear", "#2b2b3a", command("/clear")),
            key("/compact", "#2b2b3a", command("/compact")),
            key("New\nClaude", "#d97757", Action::Open { target: "wt.exe".into(), args: "claude".into() }),
        ],
        // The right knob moves through Claude Code selection menus; pressing it confirms.
        encoders: BTreeMap::from([(
            "2".to_string(),
            EncoderConfig { left: hotkey("Up"), right: hotkey("Down"), press: hotkey("Enter") },
        )]),
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            brightness: 70,
            buttons: vec![Action::ProfilePrev, Action::ProfileNext, Action::ScreenToggle],
            encoders: vec![
                EncoderConfig {
                    left: Action::SystemVolume { step: -2 },
                    right: Action::SystemVolume { step: 2 },
                    press: Action::SystemMute,
                },
                EncoderConfig {
                    left: Action::AppVolume { step: -5 },
                    right: Action::AppVolume { step: 5 },
                    press: Action::AppMute,
                },
                EncoderConfig {
                    left: Action::Media { key: MediaKey::Prev },
                    right: Action::Media { key: MediaKey::Next },
                    press: Action::Media { key: MediaKey::PlayPause },
                },
            ],
            profiles: vec![
                Profile {
                    name: "Work".into(),
                    keys: vec![
                        key("Mic", "#2d6a4f", Action::MicToggle),
                        key("Terminal", "#2b2b3a", open("wt.exe")),
                        key("VS Code", "#0065a9", open("code")),
                        key("Jira", "#0052cc", open("https://YOUR-COMPANY.atlassian.net")),
                        key("Grafana", "#f46800", open("https://YOUR-GRAFANA")),
                        key("Screen\nshot", "#5c6370", Action::Hotkey { keys: "Win+Shift+S".into() }),
                    ],
                    encoders: BTreeMap::new(),
                },
                Profile {
                    name: "Media".into(),
                    keys: vec![
                        key("Play\nPause", "#1db954", Action::Media { key: MediaKey::PlayPause }),
                        key("Prev", "#2b2b3a", Action::Media { key: MediaKey::Prev }),
                        key("Next", "#2b2b3a", Action::Media { key: MediaKey::Next }),
                        key("Mic", "#2d6a4f", Action::MicToggle),
                        key("Mute", "#5c6370", Action::SystemMute),
                        key("Lock", "#8b1e3f", Action::Lock),
                    ],
                    encoders: BTreeMap::new(),
                },
                claude_profile(),
            ],
        }
    }
}

impl Config {
    /// Pads or trims the lists to match what the device has.
    pub fn normalize(&mut self) {
        self.brightness = self.brightness.min(100);
        if self.profiles.is_empty() {
            self.profiles.push(Profile {
                name: "Profile 1".into(),
                keys: vec![],
                encoders: BTreeMap::new(),
            });
        }
        for p in &mut self.profiles {
            p.keys.resize_with(LCD_KEYS, KeyConfig::default);
            p.encoders.retain(|k, _| k.parse::<usize>().is_ok_and(|i| i < ENCODERS));
        }
        self.buttons.resize_with(SIDE_BUTTONS, Action::default);
        self.encoders.resize_with(ENCODERS, EncoderConfig::default);
    }

    /// Loads the configuration; if the file does not exist, writes the defaults.
    pub fn load_or_create(path: &Path) -> Result<Config, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let mut cfg: Config =
                    toml::from_str(&text).map_err(|e| format!("Error in {}: {e}", path.display()))?;
                cfg.normalize();
                Ok(cfg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let cfg = Config::default();
                cfg.save(path)?;
                Ok(cfg)
            }
            Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
        }
    }

    /// Writes to a temporary file first, so a crash never leaves a half-written config.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}
