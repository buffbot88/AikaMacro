mod config;
mod macro_engine;
#[cfg(target_os = "windows")]
mod ui;

use anyhow::{Context, Result};
use config::{config_path, load, save};
use macro_engine::{run as run_macro, MacroAction};
use std::{
    env,
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::Duration,
};

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
    #[cfg(not(target_os = "windows"))]
    {
        anyhow::bail!("osk-typer only supports Windows");
    }
    #[cfg(target_os = "windows")]
    {
        let mut args = env::args().skip(1);
        if args.next().is_none() {
            return ui::run(load(&config_path())?);
        }
        let text = env::args().nth(1).context("missing text")?;
        let config = load(&config_path())?;
        run(&text, &config)
    }
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn configure() -> Result<()> {
    let path = config_path();
    let mut c = load(&path)?;
    println!("Configuration file: {}", path.display());
    for i in 0..8 {
        let label = prompt(&format!("Slot {} key (blank to skip): ", i + 1))?;
        if label.is_empty() {
            c.slots[i].enabled = false;
            c.slots[i].key = None;
            continue;
        }
        let ctrl = prompt_bool("  Ctrl? [y/N]: ")?;
        let shift = prompt_bool("  Shift? [y/N]: ")?;
        c.slots[i].enabled = true;
        c.slots[i].key = Some(config::KeyBinding { label, ctrl, shift });
        c.slots[i].press_duration_ms = prompt_u64("  Press duration ms [50]: ", 50)?;
        c.slots[i].release_delay_ms = prompt_u64("  Release delay ms [50]: ", 50)?;
        c.slots[i].interval_ms = prompt_u64("  Interval ms [1000]: ", 1000)?;
    }
    save(&path, &c)?;
    println!("Saved.");
    Ok(())
}
#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn prompt(s: &str) -> Result<String> {
    use std::io::{self, Write};
    print!("{s}");
    io::stdout().flush()?;
    let mut v = String::new();
    io::stdin().read_line(&mut v)?;
    Ok(v.trim().into())
}
#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn prompt_bool(s: &str) -> Result<bool> {
    Ok(matches!(
        prompt(s)?.to_ascii_lowercase().as_str(),
        "y" | "yes" | "true"
    ))
}
#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn prompt_u64(s: &str, d: u64) -> Result<u64> {
    let v = prompt(s)?;
    if v.is_empty() {
        Ok(d)
    } else {
        v.parse().context("timer must be a non-negative integer")
    }
}

#[cfg(target_os = "windows")]
fn run(text: &str, config: &config::AppConfig) -> Result<()> {
    let (osk, notepad) = unsafe {
        (
            find_window("On-Screen Keyboard", "OSKMainClass")
                .context("launch On-Screen Keyboard first")?,
            find_window("Notepad", "Notepad").context("launch Notepad first")?,
        )
    };
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
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_run = stop.clone();
    run_macro(config, stop_for_run, |action| unsafe {
        match action {
            MacroAction::ModifierDown(k) => click_button(osk, k, 50).is_ok(),
            MacroAction::ModifierUp(k) => click_button(osk, k, 0).is_ok(),
            MacroAction::KeyDown(k) => click_button(osk, k, 50).is_ok(),
            MacroAction::KeyUp(k) => click_button(osk, k, 0).is_ok(),
            _ => true,
        }
    });
    Ok(())
}
#[cfg(target_os = "windows")]
unsafe fn find_window(title: &str, class_name: &str) -> Option<HWND> {
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
    EnumWindows(Some(cb), LPARAM(&mut s as *mut _ as isize)).ok()?;
    s.result
}
#[cfg(target_os = "windows")]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
#[cfg(target_os = "windows")]
unsafe fn click_button(parent: HWND, label: &str, hold: u64) -> Result<()> {
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
