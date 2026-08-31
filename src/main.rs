#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui::{self, Color32, RichText, Rounding, Stroke, Vec2, Sense};
use serde::{Deserialize, Serialize};
use std::{fs, sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Arc, RwLock}, thread, time::Duration};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_P, VK_SHIFT, VK_S};

const BG: Color32 = Color32::from_rgb(0x17, 0x1B, 0x20);
const PANEL: Color32 = Color32::from_rgb(0x1D, 0x22, 0x28);
const ROW_ALT: Color32 = Color32::from_rgb(0x20, 0x26, 0x2C);
const INPUT_BG: Color32 = Color32::from_rgb(0x25, 0x2B, 0x31);
const BORDER: Color32 = Color32::from_rgb(0x34, 0x3C, 0x45);
const TEXT: Color32 = Color32::from_rgb(0xF1, 0xF4, 0xF7);
const DIM: Color32 = Color32::from_rgb(0x9D, 0xA7, 0xB1);
const ACCENT: Color32 = Color32::from_rgb(0x24, 0x97, 0xE8);
const DISABLED: Color32 = Color32::from_rgb(0x66, 0x71, 0x7C);
const SUCCESS: Color32 = Color32::from_rgb(0x48, 0xC7, 0x8E);
const NEUTRAL: Color32 = Color32::from_rgb(0x87, 0x91, 0x9B);
const ERROR_COLOR: Color32 = Color32::from_rgb(0xE0, 0x60, 0x60);
const SLOT_COUNT: usize = 8;
const CONFIG_PATH: &str = "config.json";
const MIN_INTERVAL: u64 = 100;
const MIN_PRESS: u64 = 1;
const MIN_RELEASE: u64 = 1;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Skill { enabled: bool, key: String, interval_ms: u64, press_ms: u64, release_ms: u64 }
impl Default for Skill { fn default() -> Self { Self { enabled: false, key: "1".into(), interval_ms: 1000, press_ms: 50, release_ms: 50 } } }
#[derive(Clone, Serialize, Deserialize)]
struct Config { skills: Vec<Skill> }
struct App { skills: Arc<RwLock<Vec<Skill>>>, running: Arc<AtomicBool>, generation: Arc<AtomicU64>, status: Arc<RwLock<String>>, hotkeys_started: bool }
impl Default for App {
    fn default() -> Self {
        let skills = (0..SLOT_COUNT).map(|i| Skill { key: (i + 1).to_string(), ..Default::default() }).collect();
        Self { skills: Arc::new(RwLock::new(skills)), running: Arc::new(AtomicBool::new(false)), generation: Arc::new(AtomicU64::new(0)), status: Arc::new(RwLock::new("Stopped".into())), hotkeys_started: false }
    }
}

#[derive(Clone, Copy)]
struct KeyStroke { modifiers: [Option<u16>; 3], key: u16 }
fn parse_key(value: &str) -> Option<u16> {
    let key = value.trim().to_ascii_uppercase();
    if key.len() == 1 { let b = key.as_bytes()[0]; return match b { b'A'..=b'Z' | b'0'..=b'9' => Some(u16::from(b)), _ => None }; }
    if let Some(n) = key.strip_prefix('F').and_then(|v| v.parse::<u16>().ok()) { return (1..=12).contains(&n).then_some(0x6F + n); }
    match key.as_str() {
        "SPACE" => Some(0x20), "ENTER" | "RETURN" => Some(0x0D), "TAB" => Some(0x09), "ESC" | "ESCAPE" => Some(0x1B), "BACKSPACE" | "BACK" => Some(0x08),
        "LEFT" => Some(0x25), "UP" => Some(0x26), "RIGHT" => Some(0x27), "DOWN" => Some(0x28), "INSERT" => Some(0x2D), "DELETE" | "DEL" => Some(0x2E),
        "HOME" => Some(0x24), "END" => Some(0x23), "PAGEUP" | "PGUP" => Some(0x21), "PAGEDOWN" | "PGDN" => Some(0x22), _ => None,
    }
}
fn parse_keystroke(value: &str) -> Option<KeyStroke> {
    let mut modifiers = [None; 3];
    let parts: Vec<_> = value.split('+').map(str::trim).filter(|v| !v.is_empty()).collect();
    let base = *parts.last()?;
    for part in &parts[..parts.len().saturating_sub(1)] {
        let name = part.to_ascii_uppercase();
        let slot = match name.as_str() { "CTRL" | "CONTROL" => &mut modifiers[0], "ALT" => &mut modifiers[1], "SHIFT" => &mut modifiers[2], _ => return None };
        if slot.is_some() { return None; }
        *slot = Some(match name.as_str() { "CTRL" | "CONTROL" => VK_CONTROL.0, "ALT" => VK_MENU.0, "SHIFT" => VK_SHIFT.0, _ => unreachable!() });
    }
    Some(KeyStroke { modifiers, key: parse_key(base)? })
}

fn send_key(vk: u16, key_up: bool) -> Result<(), String> {
    unsafe {
        let input = INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: 0, dwFlags: if key_up { KEYEVENTF_KEYUP } else { Default::default() }, time: 0, dwExtraInfo: 0 } } };
        let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        if sent == 1 { Ok(()) } else { Err(format!("SendInput sent {sent}/1 events")) }
    }
}
fn send_keystroke(key: KeyStroke, press: u64, release: u64) -> Result<(), String> {
    for modifier in key.modifiers.into_iter().flatten() { send_key(modifier, false)?; }
    send_key(key.key, false)?; thread::sleep(Duration::from_millis(press.max(MIN_PRESS))); send_key(key.key, true)?;
    for modifier in key.modifiers.into_iter().rev().flatten() { send_key(modifier, true)?; }
    thread::sleep(Duration::from_millis(release.max(MIN_RELEASE))); Ok(())
}

fn target_running() -> Result<bool, String> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?;
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        if Process32FirstW(snapshot, &mut entry).is_ok() { loop {
            let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case("AikaTK.exe") || name.eq_ignore_ascii_case("AIKATK.exe") { found = true; break; }
            if Process32NextW(snapshot, &mut entry).is_err() { break; }
        }}
        let _ = CloseHandle(snapshot); Ok(found)
    }
}
fn set_status(status: &Arc<RwLock<String>>, value: impl Into<String>) { if let Ok(mut s) = status.write() { *s = value.into(); } }
fn stop_macro(running: &AtomicBool, generation: &AtomicU64) { running.store(false, Ordering::Release); generation.fetch_add(1, Ordering::AcqRel); }


#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Skill {
    enabled: bool,
    key: String,
    interval_ms: u64,
    press_ms: u64,
    release_ms: u64,
}

impl Default for Skill {
    fn default() -> Self {
        Self { enabled: false, key: "1".into(), interval_ms: 1000, press_ms: 50, release_ms: 50 }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Config {
    skills: Vec<Skill>,
}

struct App {
    skills: Arc<RwLock<Vec<Skill>>>,
    running: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    status: Arc<RwLock<String>>,
    hotkeys_started: bool,
}

impl Default for App {
    fn default() -> Self {
        let skills = (0..SLOT_COUNT)
            .map(|i| Skill { key: (i + 1).to_string(), ..Default::default() })
            .collect();
        Self {
            skills: Arc::new(RwLock::new(skills)),
            running: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
            status: Arc::new(RwLock::new("Stopped".into())),
            hotkeys_started: false,
        }
    }
}

#[derive(Clone, Copy)]
struct KeyStroke {
    modifiers: [Option<u16>; 3],
    key: u16,
}

fn parse_key(value: &str) -> Option<u16> {
    let key = value.trim().to_ascii_uppercase();
    if key.len() == 1 {
        let byte = key.as_bytes()[0];
        return match byte {
            b'A'..=b'Z' | b'0'..=b'9' => Some(u16::from(byte)),
            _ => None,
        };
    }
    if let Some(number) = key.strip_prefix('F').and_then(|v| v.parse::<u16>().ok()) {
        return (1..=12).contains(&number).then_some(0x6F + number);
    }
    match key.as_str() {
        "SPACE" => Some(0x20),
        "ENTER" | "RETURN" => Some(0x0D),
        "TAB" => Some(0x09),
        "ESC" | "ESCAPE" => Some(0x1B),
        "BACKSPACE" | "BACK" => Some(0x08),
        "LEFT" => Some(0x25),
        "UP" => Some(0x26),
        "RIGHT" => Some(0x27),
        "DOWN" => Some(0x28),
        "INSERT" => Some(0x2D),
        "DELETE" | "DEL" => Some(0x2E),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PAGEUP" | "PGUP" => Some(0x21),
        "PAGEDOWN" | "PGDN" => Some(0x22),
        _ => None,
    }
}

fn parse_keystroke(value: &str) -> Option<KeyStroke> {
    let mut modifiers = [None; 3];
    let parts: Vec<_> = value.split('+').map(str::trim).filter(|v| !v.is_empty()).collect();
    let base = *parts.last()?;
    for part in &parts[..parts.len().saturating_sub(1)] {
        let name = part.to_ascii_uppercase();
        let slot = match name.as_str() {
            "CTRL" | "CONTROL" => &mut modifiers[0],
            "ALT" => &mut modifiers[1],
            "SHIFT" => &mut modifiers[2],
            _ => return None,
        };
        if slot.is_some() { return None; }
        *slot = Some(match name.as_str() {
            "CTRL" | "CONTROL" => VK_CONTROL.0,
            "ALT" => VK_MENU.0,
            "SHIFT" => VK_SHIFT.0,
            _ => unreachable!(),
        });
    }
    Some(KeyStroke { modifiers, key: parse_key(base)? })
}


fn send_key(vk: u16, key_up: bool) -> Result<(), String> {
    unsafe {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: if key_up { KEYEVENTF_KEYUP } else { Default::default() },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        if sent == 1 { Ok(()) } else { Err(format!("SendInput sent {sent}/1 events")) }
    }
}

fn send_keystroke(stroke: KeyStroke, press_ms: u64, release_ms: u64) -> Result<(), String> {
    for modifier in stroke.modifiers.into_iter().flatten() { send_key(modifier, false)?; }
    send_key(stroke.key, false)?;
    thread::sleep(Duration::from_millis(press_ms.max(MIN_PRESS)));
    send_key(stroke.key, true)?;
    for modifier in stroke.modifiers.into_iter().rev().flatten() { send_key(modifier, true)?; }
    thread::sleep(Duration::from_millis(release_ms.max(MIN_RELEASE)));
    Ok(())
}

fn target_running() -> Result<bool, String> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?;
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if name.eq_ignore_ascii_case("AikaTK.exe") || name.eq_ignore_ascii_case("AIKATK.exe") { found = true; break; }
                if Process32NextW(snapshot, &mut entry).is_err() { break; }
            }
        }
        let _ = CloseHandle(snapshot);
        Ok(found)
    }
}

fn set_status(status: &Arc<RwLock<String>>, value: impl Into<String>) {
    if let Ok(mut current) = status.write() { *current = value.into(); }
}

fn stop_macro(running: &AtomicBool, generation: &AtomicU64) {
    running.store(false, Ordering::Release);
    generation.fetch_add(1, Ordering::AcqRel);
}

fn start_macro(skills: &Arc<RwLock<Vec<Skill>>>, running: &Arc<AtomicBool>, generation: &Arc<AtomicU64>, status: &Arc<RwLock<String>>) {
    if running.swap(true, Ordering::AcqRel) { return; }
    let current_generation = generation.fetch_add(1, Ordering::AcqRel) + 1;
    for slot in 0..SLOT_COUNT {
        let skills = Arc::clone(skills);
        let running = Arc::clone(running);
        let generation = Arc::clone(generation);
        let status = Arc::clone(status);
        thread::spawn(move || {
            while running.load(Ordering::Acquire) && generation.load(Ordering::Acquire) == current_generation {
                let Some(skill) = skills.read().ok().and_then(|items| items.get(slot).cloned()) else { break; };
                if !skill.enabled { thread::sleep(Duration::from_millis(50)); continue; }
                let Some(stroke) = parse_keystroke(&skill.key) else {
                    set_status(&status, format!("Invalid key in slot {}: {}", slot + 1, skill.key));
                    thread::sleep(Duration::from_millis(250)); continue;
                };
                match target_running() {
                    Ok(true) => match send_keystroke(stroke, skill.press_ms, skill.release_ms) {
                        Ok(()) => { set_status(&status, format!("Running; sent slot {}", slot + 1)); thread::sleep(Duration::from_millis(skill.interval_ms.max(MIN_INTERVAL))); }
                        Err(error) => { set_status(&status, format!("Input failed in slot {}: {error}", slot + 1)); thread::sleep(Duration::from_millis(500)); }
                    },
                    Ok(false) => { set_status(&status, "Waiting: AikaTK.exe or AIKATK.exe is not running"); thread::sleep(Duration::from_millis(250)); }
                    Err(error) => { set_status(&status, format!("Process check failed: {error}")); thread::sleep(Duration::from_millis(500)); }
                }
            }
        });
    }
}

fn save_config(skills: &Arc<RwLock<Vec<Skill>>>) -> Result<(), String> {
    let items = skills.read().map_err(|_| "Could not read settings".to_owned())?;
    let json = serde_json::to_string_pretty(&Config { skills: items.clone() }).map_err(|e| e.to_string())?;
    fs::write(CONFIG_PATH, json).map_err(|e| e.to_string())
}

fn load_config(skills: &Arc<RwLock<Vec<Skill>>>) -> Result<(), String> {
    let json = fs::read_to_string(CONFIG_PATH).map_err(|e| e.to_string())?;
    let config: Config = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    if config.skills.len() != SLOT_COUNT { return Err(format!("Expected {SLOT_COUNT} slots")); }
    *skills.write().map_err(|_| "Could not update settings".to_owned())? = config.skills;
    Ok(())
}
