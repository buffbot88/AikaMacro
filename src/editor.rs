use crate::config::{KeyBinding, MacroSlot};
use anyhow::{Context, Result};
use std::{ffi::c_void, mem::size_of};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, GetWindowTextW,
            LoadCursorW, PostMessageW, RegisterClassExW, SendMessageW, SetWindowLongPtrW,
            ShowWindow, CREATESTRUCTW, GWLP_USERDATA, HMENU, IDC_ARROW, MB_ICONERROR, MB_OK,
            SW_SHOW, WM_COMMAND, WM_DESTROY, WM_NCCREATE, WNDCLASSEXW, WS_CAPTION, WS_CHILD,
            WS_EX_CLIENTEDGE, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
        },
    },
};

pub const WM_EDITOR_RESULT: u32 = 0x8004;
pub const WM_EDITOR_CLOSED: u32 = 0x8005;

const DONE: usize = 1;
const CANCEL: usize = 2;
const KEY: usize = 3;
const CTRL: usize = 4;
const SHIFT: usize = 5;
const INTERVAL: usize = 6;
const PRESS: usize = 7;
const RELEASE: usize = 8;

struct State {
    parent: HWND,
    index: usize,
    slot: MacroSlot,
    controls: [HWND; 6],
}

#[link(name = "user32")]
unsafe extern "system" {
    fn EnableWindow(hwnd: *mut c_void, enable: i32) -> i32;
}

pub unsafe fn open(parent: HWND, index: usize, slot: MacroSlot) -> Result<HWND> {
    let instance = GetModuleHandleW(None)?;
    let class = wide("OskMacroEditor");
    let _ = RegisterClassExW(&WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: instance.into(),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    });
    let state = Box::new(State {
        parent,
        index,
        slot,
        controls: [HWND::default(); 6],
    });
    let state_ptr = Box::into_raw(state);
    let hwnd = match CreateWindowExW(
        Default::default(),
        PCWSTR(class.as_ptr()),
        PCWSTR(wide(&format!("Slot {}", index + 1)).as_ptr()),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        0,
        0,
        360,
        300,
        Some(parent),
        None,
        Some(instance.into()),
        Some(state_ptr as *const _),
    ) {
        Ok(hwnd) => hwnd,
        Err(error) => {
            drop(Box::from_raw(state_ptr));
            return Err(error.into());
        }
    };
    EnableWindow(parent.0, 0);
    let _ = ShowWindow(hwnd, SW_SHOW);
    Ok(hwnd)
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    if msg == WM_NCCREATE {
        let cs = &*(lp.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let state = &mut *ptr;
    match msg {
        0x0001 => {
            create_controls(state, hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            match wp.0 & 0xffff {
                DONE => match read_slot(state) {
                    Ok(slot) => {
                        let result = Box::new((state.index, slot));
                        let _ = PostMessageW(
                            Some(state.parent),
                            WM_EDITOR_RESULT,
                            WPARAM(0),
                            LPARAM(Box::into_raw(result) as isize),
                        );
                        DestroyWindow(hwnd).ok();
                    }
                    Err(error) => {
                        show_error(hwnd, &error.to_string());
                    }
                },
                CANCEL => {
                    DestroyWindow(hwnd).ok();
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            EnableWindow(state.parent.0, 1);
            let _ = PostMessageW(Some(state.parent), WM_EDITOR_CLOSED, WPARAM(0), LPARAM(0));
            drop(Box::from_raw(ptr));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn create_controls(state: &mut State, parent: HWND) {
    for (label, y) in [
        ("Key", 18),
        ("Modifiers", 52),
        ("Interval (ms)", 86),
        ("Press duration (ms)", 120),
        ("Release delay (ms)", 154),
    ] {
        let _ = control(parent, "STATIC", label, 0, 14, y, 145, 20);
    }
    state.controls[0] = edit(
        parent,
        state
            .slot
            .key
            .as_ref()
            .map(|key| key.label.as_str())
            .unwrap_or(""),
        KEY,
        false,
        165,
        14,
        165,
        24,
    );
    state.controls[1] = check(
        parent,
        "Ctrl",
        CTRL,
        state.slot.key.as_ref().map(|key| key.ctrl).unwrap_or(false),
        165,
        48,
    );
    state.controls[2] = check(
        parent,
        "Shift",
        SHIFT,
        state
            .slot
            .key
            .as_ref()
            .map(|key| key.shift)
            .unwrap_or(false),
        245,
        48,
    );
    state.controls[3] = edit(
        parent,
        &state.slot.interval_ms.to_string(),
        INTERVAL,
        true,
        165,
        82,
        165,
        24,
    );
    state.controls[4] = edit(
        parent,
        &state.slot.press_duration_ms.to_string(),
        PRESS,
        true,
        165,
        116,
        165,
        24,
    );
    state.controls[5] = edit(
        parent,
        &state.slot.release_delay_ms.to_string(),
        RELEASE,
        true,
        165,
        150,
        165,
        24,
    );
    let _ = button(parent, "Done", DONE, 165, 202);
    let _ = button(parent, "Cancel", CANCEL, 255, 202);
}

unsafe fn control(
    parent: HWND,
    class: &str,
    text: &str,
    id: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> HWND {
    CreateWindowExW(
        Default::default(),
        PCWSTR(wide(class).as_ptr()),
        PCWSTR(wide(text).as_ptr()),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        w,
        h,
        Some(parent),
        if id == 0 {
            None
        } else {
            Some(HMENU(id as *mut c_void))
        },
        None,
        None,
    )
    .unwrap()
}

unsafe fn edit(
    parent: HWND,
    text: &str,
    id: usize,
    numeric: bool,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> HWND {
    let numeric_style = if numeric { 0x2000 } else { 0 };
    CreateWindowExW(
        WS_EX_CLIENTEDGE,
        w!("EDIT"),
        PCWSTR(wide(text).as_ptr()),
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(numeric_style),
        x,
        y,
        w,
        h,
        Some(parent),
        Some(HMENU(id as *mut c_void)),
        None,
        None,
    )
    .unwrap()
}

unsafe fn check(parent: HWND, text: &str, id: usize, checked: bool, x: i32, y: i32) -> HWND {
    let hwnd = CreateWindowExW(
        Default::default(),
        w!("BUTTON"),
        PCWSTR(wide(text).as_ptr()),
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(0x0003),
        x,
        y,
        75,
        24,
        Some(parent),
        Some(HMENU(id as *mut c_void)),
        None,
        None,
    )
    .unwrap();
    SendMessageW(
        hwnd,
        0x00F1,
        Some(WPARAM(checked as usize)),
        Some(LPARAM(0)),
    );
    hwnd
}

unsafe fn button(parent: HWND, text: &str, id: usize, x: i32, y: i32) -> HWND {
    control(parent, "BUTTON", text, id, x, y, 80, 28)
}

unsafe fn read_slot(state: &State) -> Result<MacroSlot> {
    let key = text(state.controls[0], 128);
    if key.is_empty() {
        anyhow::bail!("key cannot be empty")
    }
    let timer = |hwnd: HWND| -> Result<u64> {
        let value: u64 = text(hwnd, 32)
            .parse()
            .context("timer must be a non-negative integer")?;
        if value > 86_400_000 {
            anyhow::bail!("timer must not exceed 86,400,000 ms")
        }
        Ok(value)
    };
    let checked = |hwnd: HWND| SendMessageW(hwnd, 0x00F0, Some(WPARAM(0)), Some(LPARAM(0))).0 != 0;
    Ok(MacroSlot {
        enabled: state.slot.enabled,
        key: Some(parse_binding(
            &key,
            checked(state.controls[1]),
            checked(state.controls[2]),
        )),
        interval_ms: timer(state.controls[3])?,
        press_duration_ms: timer(state.controls[4])?,
        release_delay_ms: timer(state.controls[5])?,
    })
}

unsafe fn text(hwnd: HWND, capacity: usize) -> String {
    let mut buffer = vec![0u16; capacity];
    GetWindowTextW(hwnd, &mut buffer);
    String::from_utf16_lossy(&buffer)
        .trim_matches(char::from(0))
        .trim()
        .to_string()
}

unsafe fn show_error(parent: HWND, text: &str) {
    let _ = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
        Some(parent),
        PCWSTR(wide(text).as_ptr()),
        PCWSTR(wide("Invalid slot settings").as_ptr()),
        MB_OK | MB_ICONERROR,
    );
}

fn parse_binding(value: &str, mut ctrl: bool, mut shift: bool) -> KeyBinding {
    let mut parts = value.split('+').map(str::trim).collect::<Vec<_>>();
    while let Some(prefix) = parts.first().map(|part| part.to_ascii_lowercase()) {
        match prefix.as_str() {
            "ctrl" | "control" => {
                ctrl = true;
                parts.remove(0);
            }
            "shift" => {
                shift = true;
                parts.remove(0);
            }
            _ => break,
        }
    }
    let label = parts.join("+");
    KeyBinding {
        label: if label.is_empty() {
            value.trim().to_string()
        } else {
            label
        },
        ctrl,
        shift,
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
