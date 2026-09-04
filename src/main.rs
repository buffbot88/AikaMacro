#![cfg_attr(all(target_os = "windows", not(test)), windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
mod accessibility;
mod config;
#[cfg(target_os = "windows")]
mod editor;
mod logger;
mod macro_engine;
#[cfg(target_os = "windows")]
mod osk;
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
use windows::Win32::{
    Foundation::{HANDLE, HWND, LPARAM},
    System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
    System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
    UI::{
        Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_CONTROL},
        WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowTextW, IsWindowVisible, SetForegroundWindow,
            ShowWindow, SW_RESTORE,
        },
    },
};
#[cfg(target_os = "windows")]
const CF_UNICODETEXT: u32 = 13;

fn main() -> Result<()> {
    let config_file = config_path();
    let config = load(&config_file);
    let logging_enabled = config.as_ref().map(|value| value.logging).unwrap_or(true);
    let log = logger::init(logging_enabled, config::log_path());
    log.log(format!(
        "starting osk-typer; logging_enabled={logging_enabled}"
    ));
    if let Err(error) = &config {
        log.log(format!("configuration load failed: {error:#}"));
    }
    #[cfg(not(target_os = "windows"))]
    {
        anyhow::bail!("osk-typer only supports Windows");
    }
    #[cfg(target_os = "windows")]
    {
        let mut args = env::args().skip(1);
        let config = match config {
            Ok(config) => config,
            Err(error) => {
                report_startup_error(&error);
                return Err(error);
            }
        };
        if args.next().is_none() {
            logger::log("launching desktop UI");
            let result = ui::run(config);
            if let Err(error) = &result {
                logger::log(format!("desktop UI exited with error: {error:#}"));
                report_startup_error(error);
            }
            return result;
        }
        let text = match env::args().nth(1).context("missing text") {
            Ok(text) => text,
            Err(error) => {
                report_startup_error(&error);
                return Err(error);
            }
        };
        logger::log("launching CLI macro flow");
        let result = run(&text, &config);
        if let Err(error) = &result {
            logger::log(format!("CLI macro flow failed: {error:#}"));
            report_startup_error(error);
        }
        result
    }
}

#[cfg(target_os = "windows")]
fn report_startup_error(error: &anyhow::Error) {
    use windows::{
        core::PCWSTR,
        Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK},
    };
    let message = format!("{error:#}\n\nDetails were written to osk-macro.log.");
    let title = "OSK Macro could not start";
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(wide(&message).as_ptr()),
            PCWSTR(wide(title).as_ptr()),
            MB_OK | MB_ICONERROR,
        );
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
    let osk_session = osk::Session::start()?;
    let osk = osk_session.hwnd;
    let notepad = unsafe { find_window("Notepad", "Notepad").context("launch Notepad first")? };
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
    let mut failure = None;
    let completed = run_macro(config, stop_for_run, |action| match action {
        MacroAction::ModifierDown(k) => match crate::accessibility::invoke_control(osk, k) {
            Ok(()) => true,
            Err(error) => {
                failure = Some(format!("{error:#}"));
                false
            }
        },
        MacroAction::ModifierUp(k) => match crate::accessibility::invoke_control(osk, k) {
            Ok(()) => true,
            Err(error) => {
                failure = Some(format!("{error:#}"));
                false
            }
        },
        MacroAction::KeyDown(k) => match crate::accessibility::invoke_control(osk, k) {
            Ok(()) => true,
            Err(error) => {
                failure = Some(format!("{error:#}"));
                false
            }
        },
        MacroAction::KeyUp(_) | MacroAction::Hold(_) | MacroAction::Delay(_) => true,
    });
    drop(osk_session);
    if !completed {
        anyhow::bail!(failure.unwrap_or_else(|| "OSK input failed".to_string()));
    }
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
