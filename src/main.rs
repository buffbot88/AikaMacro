use anyhow::{bail, Context, Result};
use std::{
    env,
    io::{self, Write},
    thread,
    time::Duration,
};

#[derive(Clone, Debug)]
struct Config {
    keys: Vec<ProgrammableKey>,
}
#[derive(Clone, Debug)]
struct ProgrammableKey {
    key: String,
    ctrl: bool,
    shift: bool,
    press_ms: u64,
    hold_ms: u64,
    between_ms: u64,
}

#[cfg(target_os = "windows")]
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{HANDLE, HWND, LPARAM, RECT},
        System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
        UI::{
            Input::KeyboardAndMouse::{
                keybd_event, SendInput, INPUT, INPUT_0, INPUT_MOUSE, KEYEVENTF_KEYUP,
                MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
                MOUSEINPUT, VK_CONTROL,
            },
            WindowsAndMessaging::{
                EnumWindows, FindWindowExW, GetClassNameW, GetWindowRect, GetWindowTextW,
                IsWindowVisible, SetForegroundWindow, ShowWindow, SW_RESTORE,
            },
        },
    },
};
#[cfg(target_os = "windows")]
const CF_UNICODETEXT: u32 = 13;

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let text = args.next();
    let delay_ms = args
        .next()
        .map(|v| v.parse::<u64>().context("delay must be an integer"))
        .transpose()?
        .unwrap_or(35);
    #[cfg(target_os = "windows")]
    {
        let config = load_config()?;
        if let Some(text) = text {
            return run(&text, delay_ms, &config);
        }
        println!(
            "Configured {} key(s). Use: osk-typer <text>",
            config.keys.len()
        );
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (text, delay_ms);
        bail!("osk-typer only supports Windows");
    }
}

#[cfg(target_os = "windows")]
fn load_config() -> Result<Config> {
    let path = config_path();
    if let Ok(contents) = std::fs::read_to_string(&path) {
        return parse_config(&contents);
    }
    println!("Configure up to 8 programmable keys. Press Enter to skip a slot.");
    let mut keys = Vec::new();
    for index in 1..=8 {
        let key = prompt(&format!("Key {index} (OSK label): "))?;
        if key.is_empty() {
            continue;
        }
        let ctrl = prompt_bool("  Ctrl? [y/N]: ")?;
        let shift = prompt_bool("  Shift? [y/N]: ")?;
        let press_ms = prompt_u64("  Press delay ms [35]: ", 35)?;
        let hold_ms = prompt_u64("  Hold delay ms [0]: ", 0)?;
        let between_ms = prompt_u64("  Between delay ms [35]: ", 35)?;
        keys.push(ProgrammableKey {
            key,
            ctrl,
            shift,
            press_ms,
            hold_ms,
            between_ms,
        });
    }
    let config = Config { keys };
    save_config(&path, &config)?;
    Ok(config)
}
#[cfg(target_os = "windows")]
fn config_path() -> std::path::PathBuf {
    env::current_exe()
        .unwrap_or_else(|_| "osk-typer.exe".into())
        .with_file_name("osk-typer.conf")
}
#[cfg(target_os = "windows")]
fn parse_config(contents: &str) -> Result<Config> {
    let mut keys = Vec::new();
    for (n, line) in contents.lines().enumerate() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let f: Vec<_> = line.split('|').collect();
        if f.len() != 6 {
            bail!("invalid config line {}", n + 1)
        }
        if keys.len() == 8 {
            bail!("at most 8 keys are supported")
        }
        keys.push(ProgrammableKey {
            key: f[0].into(),
            ctrl: parse_bool(f[1])?,
            shift: parse_bool(f[2])?,
            press_ms: f[3].parse().context("invalid press timer")?,
            hold_ms: f[4].parse().context("invalid hold timer")?,
            between_ms: f[5].parse().context("invalid between timer")?,
        });
    }
    Ok(Config { keys })
}
#[cfg(target_os = "windows")]
fn save_config(path: &std::path::Path, config: &Config) -> Result<()> {
    let mut out = String::from("# key|ctrl|shift|press_ms|hold_ms|between_ms\n");
    for k in &config.keys {
        out.push_str(&format!(
            "{}|{}|{}|{}|{}|{}\n",
            k.key, k.ctrl, k.shift, k.press_ms, k.hold_ms, k.between_ms
        ));
    }
    std::fs::write(path, out).with_context(|| format!("could not write {}", path.display()))?;
    Ok(())
}
fn prompt(message: &str) -> Result<String> {
    print!("{message}");
    io::stdout().flush()?;
    let mut v = String::new();
    io::stdin().read_line(&mut v)?;
    Ok(v.trim().into())
}
fn prompt_bool(message: &str) -> Result<bool> {
    Ok(matches!(
        prompt(message)?.to_ascii_lowercase().as_str(),
        "y" | "yes" | "true"
    ))
}
fn prompt_u64(message: &str, default: u64) -> Result<u64> {
    let v = prompt(message)?;
    if v.is_empty() {
        Ok(default)
    } else {
        v.parse().context("timer must be a non-negative integer")
    }
}
fn parse_bool(v: &str) -> Result<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" => Ok(true),
        "false" | "0" | "no" | "n" => Ok(false),
        _ => bail!("invalid boolean"),
    }
}

#[cfg(target_os = "windows")]
fn run(text: &str, delay_ms: u64, config: &Config) -> Result<()> {
    let osk = find_window("On-Screen Keyboard", "OSKMainClass")
        .context("launch On-Screen Keyboard first")?;
    let notepad = find_window("Notepad", "Notepad").context("launch Notepad first")?;
    unsafe {
        let _ = ShowWindow(notepad, SW_RESTORE);
        let _ = SetForegroundWindow(notepad);
    }
    thread::sleep(Duration::from_millis(200));
    set_clipboard(text)?;
    send_ctrl_v();
    thread::sleep(Duration::from_millis(100));
    unsafe {
        let _ = ShowWindow(osk, SW_RESTORE);
        let _ = SetForegroundWindow(osk);
    }
    thread::sleep(Duration::from_millis(200));
    for k in &config.keys {
        if k.ctrl {
            click_button(osk, "Ctrl", k.hold_ms)?;
        }
        if k.shift {
            click_button(osk, "Shift", k.hold_ms)?;
        }
        click_button(osk, &k.key, k.press_ms)?;
        thread::sleep(Duration::from_millis(k.between_ms));
    }
    thread::sleep(Duration::from_millis(delay_ms));
    Ok(())
}
#[cfg(target_os = "windows")]
fn find_window(title: &str, class_name: &str) -> Option<HWND> {
    struct Search {
        title: Vec<u16>,
        class_name: Vec<u16>,
        result: Option<HWND>,
    }
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> windows::core::BOOL {
        let s = &mut *(lp.0 as *mut Search);
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }
        let mut c = [0u16; 256];
        let cl = GetClassNameW(hwnd, &mut c) as usize;
        let mut t = [0u16; 512];
        let tl = GetWindowTextW(hwnd, &mut t) as usize;
        if c[..cl] == s.class_name[..s.class_name.len() - 1]
            || t[..tl]
                .windows(s.title.len() - 1)
                .any(|w| w == &s.title[..s.title.len() - 1])
        {
            s.result = Some(hwnd);
            return false.into();
        }
        true.into()
    }
    let mut s = Search {
        title: wide(title),
        class_name: wide(class_name),
        result: None,
    };
    unsafe {
        EnumWindows(Some(cb), LPARAM(&mut s as *mut _ as isize)).ok()?;
    }
    s.result
}
#[cfg(target_os = "windows")]
fn wide(v: &str) -> Vec<u16> {
    v.encode_utf16().chain(Some(0)).collect()
}
#[cfg(target_os = "windows")]
fn click_button(parent: HWND, label: &str, hold: u64) -> Result<()> {
    unsafe {
        let b = FindWindowExW(
            Some(parent),
            None,
            w!("Button"),
            PCWSTR(wide(label).as_ptr()),
        )?;
        let mut r = RECT::default();
        GetWindowRect(b, &mut r)?;
        let x = ((r.left + r.right) / 2).clamp(0, 65535);
        let y = ((r.top + r.bottom) / 2).clamp(0, 65535);
        let d = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: x,
                    dy: y,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_LEFTDOWN,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let u = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: x,
                    dy: y,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_LEFTUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        SendInput(&[d], std::mem::size_of::<INPUT>() as i32);
        thread::sleep(Duration::from_millis(hold));
        SendInput(&[u], std::mem::size_of::<INPUT>() as i32);
    }
    Ok(())
}
#[cfg(target_os = "windows")]
fn send_ctrl_v() {
    unsafe {
        keybd_event(VK_CONTROL.0 as u8, 0, Default::default(), 0);
        keybd_event(b'V', 0, Default::default(), 0);
        keybd_event(b'V', 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}
#[cfg(target_os = "windows")]
fn set_clipboard(text: &str) -> Result<()> {
    let wide = text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    unsafe {
        OpenClipboard(None).context("OpenClipboard failed")?;
        EmptyClipboard().context("EmptyClipboard failed")?;
        let m = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).context("GlobalAlloc failed")?;
        let p = GlobalLock(m) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        GlobalUnlock(m).ok();
        SetClipboardData(CF_UNICODETEXT, Some(HANDLE(m.0))).context("SetClipboardData failed")?;
        CloseClipboard().ok();
    }
    Ok(())
}
