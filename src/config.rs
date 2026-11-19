//! Konfiguracja kontrolera zapisywana w `config.toml`.

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

pub const LCD_KEYS: usize = 6;
pub const SIDE_BUTTONS: usize = 3;
pub const ENCODERS: usize = 3;

/// Akcja przypisana do przycisku / pokrętła.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    #[default]
    None,
    /// Otwiera aplikację, plik, folder lub URL (jak `start` w cmd).
    Open {
        target: String,
        #[serde(default)]
        args: String,
    },
    /// Skrót klawiszowy, np. "Ctrl+Shift+M".
    Hotkey { keys: String },
    /// Wpisuje tekst; `enter` dodatkowo go zatwierdza (np. polecenie w Claude Code).
    Text {
        text: String,
        #[serde(default)]
        enter: bool,
    },
    Media { key: MediaKey },
    /// Zmiana głośności systemowej o `step` punktów procentowych.
    SystemVolume { step: i32 },
    SystemMute,
    /// Zmiana głośności aplikacji, której okno jest aktywne.
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
    /// Pokrętła ustawione inaczej niż wspólne – klucz to numer pokrętła ("0" lewe,
    /// "1" górne, "2" prawe). Brak wpisu = ustawienie wspólne.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub encoders: BTreeMap<String, EncoderConfig>,
}

impl Config {
    /// Ustawienie pokrętła obowiązujące w danym profilu.
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
    /// Przyciski bez ekranu (A, B, C) – wspólne dla wszystkich profili.
    #[serde(default)]
    pub buttons: Vec<Action>,
    /// Pokrętła: lewe, górne, prawe – wspólne dla wszystkich profili.
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

/// Sterowanie Claude Code w terminalu (działa na aktywne okno).
pub fn claude_profile() -> Profile {
    let command = |text: &str| Action::Text { text: text.into(), enter: true };
    let hotkey = |keys: &str| Action::Hotkey { keys: keys.into() };
    Profile {
        name: "Claude".into(),
        keys: vec![
            key("Akceptuj", "#2d6a4f", hotkey("Enter")),
            key("Stop", "#b42318", hotkey("Esc")),
            key("Tryb", "#7c3aed", hotkey("Shift+Tab")),
            key("/clear", "#2b2b3a", command("/clear")),
            key("/compact", "#2b2b3a", command("/compact")),
            key("Nowy\nClaude", "#d97757", Action::Open { target: "wt.exe".into(), args: "claude".into() }),
        ],
        // Prawe pokrętło przewija menu wyboru w Claude Code, wciśnięcie zatwierdza.
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
                    name: "Praca".into(),
                    keys: vec![
                        key("Mikrofon", "#2d6a4f", Action::MicToggle),
                        key("Terminal", "#2b2b3a", open("wt.exe")),
                        key("VS Code", "#0065a9", open("code")),
                        key("Jira", "#0052cc", open("https://TWOJA-FIRMA.atlassian.net")),
                        key("Grafana", "#f46800", open("https://TWOJA-GRAFANA")),
                        key("Zrzut\nekranu", "#5c6370", Action::Hotkey { keys: "Win+Shift+S".into() }),
                    ],
                    encoders: BTreeMap::new(),
                },
                Profile {
                    name: "Media".into(),
                    keys: vec![
                        key("Play\nPauza", "#1db954", Action::Media { key: MediaKey::PlayPause }),
                        key("Poprz.", "#2b2b3a", Action::Media { key: MediaKey::Prev }),
                        key("Nast.", "#2b2b3a", Action::Media { key: MediaKey::Next }),
                        key("Mikrofon", "#2d6a4f", Action::MicToggle),
                        key("Wycisz", "#5c6370", Action::SystemMute),
                        key("Zablokuj", "#8b1e3f", Action::Lock),
                    ],
                    encoders: BTreeMap::new(),
                },
                claude_profile(),
            ],
        }
    }
}

impl Config {
    /// Wyrównuje liczbę elementów do tego, co ma urządzenie.
    pub fn normalize(&mut self) {
        self.brightness = self.brightness.min(100);
        if self.profiles.is_empty() {
            self.profiles.push(Profile {
                name: "Profil 1".into(),
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

    /// Wczytuje config; gdy pliku nie ma – zapisuje domyślny.
    pub fn load_or_create(path: &Path) -> Result<Config, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let mut cfg: Config =
                    toml::from_str(&text).map_err(|e| format!("Błąd w {}: {e}", path.display()))?;
                cfg.normalize();
                Ok(cfg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let cfg = Config::default();
                cfg.save(path)?;
                Ok(cfg)
            }
            Err(e) => Err(format!("Nie można odczytać {}: {e}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}
