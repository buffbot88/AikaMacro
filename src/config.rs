use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const SLOT_COUNT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    VirtualKey,
    ScanCode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBinding {
    pub label: String,
    pub ctrl: bool,
    pub shift: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroSlot {
    pub enabled: bool,
    pub key: Option<KeyBinding>,
    pub interval_ms: u64,
    pub press_duration_ms: u64,
    pub release_delay_ms: u64,
}

impl Default for MacroSlot {
    fn default() -> Self {
        Self {
            enabled: true,
            key: None,
            interval_ms: 1000,
            press_duration_ms: 50,
            release_delay_ms: 50,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppConfig {
    pub slots: [MacroSlot; SLOT_COUNT],
    pub input_mode: InputMode,
    pub logging: bool,
}
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| MacroSlot::default()),
            input_mode: InputMode::VirtualKey,
            logging: true,
        }
    }
}

pub fn log_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("osk-macro.exe"))
        .with_file_name("osk-macro.log")
}

pub fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("osk-macro.exe"))
        .with_file_name("osk-macro.conf")
}

pub fn load(path: &Path) -> Result<AppConfig> {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(e) => Err(e).with_context(|| format!("could not read {}", path.display())),
    }
}
pub fn save(path: &Path, config: &AppConfig) -> Result<()> {
    let mut out = String::from("version=1\n");
    out.push_str(&format!("logging={}\n", config.logging));
    out.push_str(&format!(
        "input_mode={}\n",
        match config.input_mode {
            InputMode::VirtualKey => "virtual-key",
            InputMode::ScanCode => "scan-code",
        }
    ));
    for (i, s) in config.slots.iter().enumerate() {
        let key = s.key.as_ref();
        out.push_str(&format!("slot.{i}.enabled={}\nslot.{i}.key={}\nslot.{i}.ctrl={}\nslot.{i}.shift={}\nslot.{i}.interval_ms={}\nslot.{i}.press_duration_ms={}\nslot.{i}.release_delay_ms={}\n",s.enabled,key.map(|k|k.label.as_str()).unwrap_or(""),key.map(|k|k.ctrl).unwrap_or(false),key.map(|k|k.shift).unwrap_or(false),s.interval_ms,s.press_duration_ms,s.release_delay_ms));
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, out)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn parse(text: &str) -> Result<AppConfig> {
    let mut config = AppConfig::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k == "logging" {
            config.logging = parse_bool(v)?;
            continue;
        }
        if k == "input_mode" {
            config.input_mode = match v {
                "scan-code" => InputMode::ScanCode,
                "virtual-key" => InputMode::VirtualKey,
                _ => bail!("invalid input mode"),
            };
            continue;
        }
        let Some(rest) = k.strip_prefix("slot.") else {
            continue;
        };
        let Some((idx, field)) = rest.split_once('.') else {
            continue;
        };
        let i: usize = idx.parse().context("invalid slot index")?;
        if i >= SLOT_COUNT {
            bail!("slot index out of range")
        };
        match field {
            "enabled" => config.slots[i].enabled = parse_bool(v)?,
            "key" => {
                if v.is_empty() {
                    config.slots[i].key = None;
                } else {
                    config.slots[i].key = Some(KeyBinding {
                        label: v.to_string(),
                        ctrl: false,
                        shift: false,
                    });
                }
            }
            "ctrl" => {
                let value = parse_bool(v)?;
                if let Some(key) = config.slots[i].key.as_mut() {
                    key.ctrl = value;
                }
            }
            "shift" => {
                let value = parse_bool(v)?;
                if let Some(key) = config.slots[i].key.as_mut() {
                    key.shift = value;
                }
            }
            "interval_ms" => config.slots[i].interval_ms = parse_timer(v)?,
            "press_duration_ms" => config.slots[i].press_duration_ms = parse_timer(v)?,
            "release_delay_ms" => config.slots[i].release_delay_ms = parse_timer(v)?,
            _ => {}
        }
    }
    Ok(config)
}
fn parse_bool(v: &str) -> Result<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" => Ok(true),
        "false" | "0" | "no" | "n" => Ok(false),
        _ => bail!("invalid boolean"),
    }
}
fn parse_timer(v: &str) -> Result<u64> {
    let n: u64 = v.parse().context("timer must be a non-negative integer")?;
    if n > 86_400_000 {
        bail!("timer is too large")
    };
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let mut c = AppConfig::default();
        c.slots[0].key = Some(KeyBinding {
            label: "F1".into(),
            ctrl: true,
            shift: false,
        });
        let mut p = std::env::temp_dir();
        p.push(format!("osk-macro-test-{}", std::process::id()));
        save(&p, &c).unwrap();
        let loaded = load(&p).unwrap();
        fs::remove_file(p).unwrap();
        assert_eq!(loaded, c);
    }
    #[test]
    fn logging_setting_round_trips() {
        let config = parse("version=1\nlogging=false\ninput_mode=scan-code\n").unwrap();
        assert!(!config.logging);
        assert_eq!(config.input_mode, InputMode::ScanCode);
    }

    #[test]
    fn rejects_ninth_slot() {
        let mut t = String::new();
        for i in 0..9 {
            t.push_str(&format!("slot.{i}.enabled=true\n"));
        }
        assert!(parse(&t).is_err());
    }
}
