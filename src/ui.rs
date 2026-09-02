use crate::{
    config::{self, AppConfig, KeyBinding, SLOT_COUNT},
    macro_engine::{self, MacroAction},
};
use anyhow::Result;
use std::{
    ffi::c_void,
    mem::size_of,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, SetBkMode,
            SetTextColor, TextOutW, PAINTSTRUCT, TRANSPARENT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE,
                MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEINPUT, VK_CONTROL,
                VK_SHIFT,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, FindWindowExW,
                GetClassNameW, GetClientRect, GetMessageW, GetWindowLongPtrW, GetWindowRect,
                GetWindowTextW, IsWindowVisible, LoadCursorW, MessageBoxW, MoveWindow,
                PostMessageW, PostQuitMessage, RegisterClassExW, SendMessageW, SetWindowLongPtrW,
                SetWindowTextW, ShowWindow, TranslateMessage, CREATESTRUCTW, CW_USEDEFAULT,
                GWLP_USERDATA, HMENU, IDC_ARROW, MB_ICONERROR, MB_OK, MINMAXINFO, MSG, SW_SHOW,
                WM_APP, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_GETMINMAXINFO, WM_HOTKEY, WM_KEYDOWN,
                WM_NCCREATE, WM_PAINT, WM_SIZE, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN,
                WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
                WS_THICKFRAME, WS_VISIBLE,
            },
        },
    },
};

const ID_START: usize = 100;
const ID_STOP: usize = 101;
const ID_SAVE: usize = 102;
const ID_LOAD: usize = 103;
const ID_MODE: usize = 104;
const ID_SLOT_BASE: usize = 200;
const ID_TIMER_BASE: usize = 300;
const ID_TOGGLE_BASE: usize = 400;
const WM_APP_DONE: u32 = WM_APP + 1;
const WM_APP_ERROR: u32 = WM_APP + 2;

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterHotKey(hwnd: *mut c_void, id: i32, modifiers: u32, key: u32) -> i32;
    fn UnregisterHotKey(hwnd: *mut c_void, id: i32) -> i32;
}
const WM_APP_OPEN_EDITOR: u32 = WM_APP + 3;

pub struct Ui {
    pub hwnd: HWND,
    model: Arc<Mutex<AppConfig>>,
    keys: [HWND; SLOT_COUNT],
    timers: [HWND; SLOT_COUNT],
    toggles: [HWND; SLOT_COUNT],
    status: HWND,
    start: HWND,
    stop: HWND,
    mode: HWND,
    stop_flag: Arc<AtomicBool>,
    capture: Option<usize>,
    last_error: Option<String>,
}

pub fn run(model: AppConfig) -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = wide("OskMacroBar");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let state = Box::new(Ui {
            hwnd: HWND::default(),
            model: Arc::new(Mutex::new(model)),
            keys: [HWND::default(); SLOT_COUNT],
            timers: [HWND::default(); SLOT_COUNT],
            toggles: [HWND::default(); SLOT_COUNT],
            status: HWND::default(),
            start: HWND::default(),
            stop: HWND::default(),
            mode: HWND::default(),
            stop_flag: Arc::new(AtomicBool::new(false)),
            capture: None,
            last_error: None,
        });
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            PCWSTR(class.as_ptr()),
            PCWSTR(wide("OSK Macro").as_ptr()),
            WS_OVERLAPPED
                | WS_CAPTION
                | WS_SYSMENU
                | WS_MINIMIZEBOX
                | WS_THICKFRAME
                | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            960,
            145,
            None,
            None,
            Some(instance.into()),
            Some(Box::into_raw(state) as *const _),
        )?;
        let _ = ShowWindow(hwnd, SW_SHOW);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Ok(())
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
    if msg == WM_NCCREATE {
        let cs = &*(lp.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        (*(cs.lpCreateParams as *mut Ui)).hwnd = hwnd;
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let ui = &mut *ptr;
    match msg {
        WM_CREATE => {
            create(ui);
            layout(ui);
            register_hotkeys(hwnd);
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let m = &mut *(lp.0 as *mut MINMAXINFO);
            m.ptMinTrackSize.x = 820;
            m.ptMinTrackSize.y = 135;
            m.ptMaxTrackSize.y = 145;
            LRESULT(0)
        }
        WM_SIZE => {
            layout(ui);
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            command(ui, wp);
            LRESULT(0)
        }
        WM_HOTKEY => {
            if wp.0 == 1 {
                start_macro(ui)
            } else if wp.0 == 2 {
                stop_macro(ui)
            };
            LRESULT(0)
        }
        WM_APP_OPEN_EDITOR => {
            open_editor(ui, wp.0);
            LRESULT(0)
        }
        WM_APP_DONE => {
            set_status(ui, "● Stopped");
            LRESULT(0)
        }
        WM_APP_ERROR => {
            let msg = Box::from_raw(lp.0 as *mut String);
            set_error(ui, *msg);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if ui.capture.is_some() {
                capture_key(ui, wp.0 as u32);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unregister_hotkeys(hwnd);
            ui.stop_flag.store(true, Ordering::Relaxed);
            drop(Box::from_raw(ptr));
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn create(ui: &mut Ui) {
    let p = ui.hwnd;
    ui.start = button(p, "▶ Start", ID_START);
    ui.stop = button(p, "■ Stop", ID_STOP);
    button(p, "Save", ID_SAVE);
    button(p, "Load", ID_LOAD);
    ui.mode = combo(p, ID_MODE);
    ui.status = label(p, "● Stopped");
    for i in 0..8 {
        ui.toggles[i] = button(p, "●", ID_TOGGLE_BASE + i);
        ui.keys[i] = edit(p, "Press", ID_SLOT_BASE + i);
        ui.timers[i] = edit(p, "1000", ID_TIMER_BASE + i);
    }
}
unsafe fn button(p: HWND, t: &str, id: usize) -> HWND {
    CreateWindowExW(
        Default::default(),
        w!("BUTTON"),
        PCWSTR(wide(t).as_ptr()),
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        90,
        28,
        Some(p),
        Some(HMENU(id as *mut c_void)),
        None,
        None,
    )
    .unwrap()
}
unsafe fn label(p: HWND, t: &str) -> HWND {
    CreateWindowExW(
        Default::default(),
        w!("STATIC"),
        PCWSTR(wide(t).as_ptr()),
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        130,
        26,
        Some(p),
        None,
        None,
        None,
    )
    .unwrap()
}
unsafe fn combo(p: HWND, id: usize) -> HWND {
    let h = CreateWindowExW(
        Default::default(),
        w!("COMBOBOX"),
        PCWSTR::null(),
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        130,
        26,
        Some(p),
        Some(HMENU(id as *mut c_void)),
        None,
        None,
    )
    .unwrap();
    SendMessageW(
        h,
        0x0143,
        Some(WPARAM(0)),
        Some(LPARAM(wide("Virtual-Key").as_ptr() as isize)),
    );
    SendMessageW(
        h,
        0x0143,
        Some(WPARAM(0)),
        Some(LPARAM(wide("Scan-Code").as_ptr() as isize)),
    );
    SendMessageW(h, 0x014E, Some(WPARAM(0)), Some(LPARAM(0)));
    h
}
unsafe fn edit(p: HWND, t: &str, id: usize) -> HWND {
    CreateWindowExW(
        WS_EX_CLIENTEDGE,
        w!("EDIT"),
        PCWSTR(wide(t).as_ptr()),
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        60,
        26,
        Some(p),
        Some(HMENU(id as *mut c_void)),
        None,
        None,
    )
    .unwrap()
}
unsafe fn layout(ui: &mut Ui) {
    let mut r = RECT::default();
    GetClientRect(ui.hwnd, &mut r).ok();
    let w = r.right;
    let _ = MoveWindow(ui.mode, w - 285, 8, 130, 26, true);
    let _ = MoveWindow(ui.status, w - 145, 10, 130, 26, true);
    for i in 0..8 {
        let x = 8 + (w - 16) * i as i32 / 8;
        let next = 8 + (w - 16) * (i as i32 + 1) / 8;
        let _ = MoveWindow(ui.toggles[i], x, 50, 24, 26, true);
        let _ = MoveWindow(ui.keys[i], x + 25, 50, 42, 26, true);
        let _ = MoveWindow(ui.timers[i], x + 70, 50, (next - x - 74).max(45), 26, true);
    }
    let _ = MoveWindow(ui.start, 8, 100, 105, 30, true);
    let _ = MoveWindow(ui.stop, 120, 100, 90, 30, true);
}
#[allow(dead_code)]
unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let brush = CreateSolidBrush(COLORREF(0x00201B17));
    let _ = FillRect(hdc, &ps.rcPaint, brush);
    let _ = DeleteObject(brush.into());
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, COLORREF(0x00F1F4F7));
    let title = wide("OSK Macro");
    let subtitle = wide("Compact Windows keyboard macro");
    let hints = wide("Ctrl+P Start  •  Ctrl+S Stop");
    let _ = TextOutW(hdc, 12, 8, &title[..title.len() - 1]);
    let _ = TextOutW(hdc, 12, 27, &subtitle[..subtitle.len() - 1]);
    let _ = TextOutW(hdc, 530, 108, &hints[..hints.len() - 1]);
    let _ = EndPaint(hwnd, &ps);
}
unsafe fn open_editor(ui: &mut Ui, index: usize) {
    if index >= SLOT_COUNT {
        return;
    }
    let slot = ui.model.lock().unwrap().slots[index].clone();
    let text = format!("Slot {}\nKey: {}\nCtrl: {}\nShift: {}\nInterval: {} ms\nPress duration: {} ms\nRelease delay: {} ms", index + 1, slot.key.as_ref().map(|k| k.label.as_str()).unwrap_or("(none)"), slot.key.as_ref().map(|k| k.ctrl).unwrap_or(false), slot.key.as_ref().map(|k| k.shift).unwrap_or(false), slot.interval_ms, slot.press_duration_ms, slot.release_delay_ms);
    MessageBoxW(
        Some(ui.hwnd),
        PCWSTR(wide(&text).as_ptr()),
        PCWSTR(wide("Slot editor").as_ptr()),
        MB_OK,
    );
}
unsafe fn command(ui: &mut Ui, wp: WPARAM) {
    let id = wp.0 & 0xffff;
    match id {
        ID_START => start_macro(ui),
        ID_STOP => stop_macro(ui),
        ID_SAVE => {
            sync(ui);
            let model = ui.model.lock().unwrap().clone();
            if let Err(e) = config::save(&config::config_path(), &model) {
                set_error(ui, e.to_string());
            }
        }
        ID_LOAD => match config::load(&config::config_path()) {
            Ok(m) => {
                *ui.model.lock().unwrap() = m;
                refresh(ui)
            }
            Err(e) => set_error(ui, e.to_string()),
        },
        id if (ID_SLOT_BASE..ID_SLOT_BASE + 8).contains(&id) => {
            let i = id - ID_SLOT_BASE;
            let _ = PostMessageW(Some(ui.hwnd), WM_APP_OPEN_EDITOR, WPARAM(i), LPARAM(0));
            ui.capture = Some(i);
            let _ = SetWindowTextW(ui.keys[i], PCWSTR(wide("Press...").as_ptr()));
            let _ = PostMessageW(Some(ui.keys[i]), 0x0007, WPARAM(0), LPARAM(0));
        }
        id if (ID_TOGGLE_BASE..ID_TOGGLE_BASE + 8).contains(&id)
            || (ID_TIMER_BASE..ID_TIMER_BASE + 8).contains(&id) =>
        {
            sync(ui)
        }
        _ => {}
    }
}
unsafe fn sync(ui: &mut Ui) {
    let mut m = ui.model.lock().unwrap();
    for i in 0..8 {
        let mut b = [0u16; 64];
        GetWindowTextW(ui.keys[i], &mut b);
        let label = String::from_utf16_lossy(&b)
            .trim_matches(char::from(0))
            .trim()
            .to_string();
        let mut t = [0u16; 32];
        GetWindowTextW(ui.timers[i], &mut t);
        m.slots[i].enabled =
            SendMessageW(ui.toggles[i], 0x00F0, Some(WPARAM(0)), Some(LPARAM(0))).0 != 0;
        m.slots[i].key = if label.is_empty() || label == "Press" || label == "Press..." {
            None
        } else {
            Some(KeyBinding {
                label,
                ctrl: false,
                shift: false,
            })
        };
        m.slots[i].interval_ms = String::from_utf16_lossy(&t)
            .trim_matches(char::from(0))
            .trim()
            .parse()
            .unwrap_or(1000);
    }
}
unsafe fn refresh(ui: &mut Ui) {
    let m = ui.model.lock().unwrap();
    for i in 0..8 {
        let _ = SetWindowTextW(
            ui.keys[i],
            PCWSTR(
                wide(
                    m.slots[i]
                        .key
                        .as_ref()
                        .map(|k| k.label.as_str())
                        .unwrap_or("Press"),
                )
                .as_ptr(),
            ),
        );
        let _ = SetWindowTextW(
            ui.timers[i],
            PCWSTR(wide(&m.slots[i].interval_ms.to_string()).as_ptr()),
        );
        SendMessageW(
            ui.toggles[i],
            0x00F1,
            Some(WPARAM(m.slots[i].enabled as usize)),
            Some(LPARAM(0)),
        );
    }
}
unsafe fn capture_key(ui: &mut Ui, value: u32) {
    let Some(i) = ui.capture.take() else { return };
    let ctrl = GetAsyncKeyState(VK_CONTROL.0 as i32) < 0;
    let shift = GetAsyncKeyState(VK_SHIFT.0 as i32) < 0;
    let label = match value {
        0x20 => "Space".into(),
        0x0D => "Enter".into(),
        0x1B => "Press".into(),
        0x70..=0x7B => format!("F{}", value - 0x6F),
        v if v < 256 => char::from_u32(v)
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_else(|| format!("VK{v}")),
        v => format!("VK{v}"),
    };
    let display = if ctrl && shift {
        format!("Ctrl+Shift+{label}")
    } else if ctrl {
        format!("Ctrl+{label}")
    } else if shift {
        format!("Shift+{label}")
    } else {
        label
    };
    let _ = SetWindowTextW(ui.keys[i], PCWSTR(wide(&display).as_ptr()));
    let mut m = ui.model.lock().unwrap();
    m.slots[i].key = Some(KeyBinding {
        label: display,
        ctrl,
        shift,
    });
}
unsafe fn start_macro(ui: &mut Ui) {
    sync(ui);
    let model = ui.model.lock().unwrap().clone();
    if !model.slots.iter().any(|s| s.enabled && s.key.is_some()) {
        set_error(ui, "Configure at least one enabled slot".into());
        return;
    }
    let osk = match find_window("On-Screen Keyboard", "OSKMainClass") {
        Some(v) => v,
        None => {
            set_error(ui, "Launch On-Screen Keyboard first".into());
            return;
        }
    };
    ui.stop_flag.store(false, Ordering::Relaxed);
    set_status(ui, "● Running");
    set_enabled(ui.start, false);
    set_enabled(ui.stop, true);
    let stop = ui.stop_flag.clone();
    let hwnd = ui.hwnd;
    let osk_raw = osk.0 as usize;
    let hwnd_raw = hwnd.0 as usize;
    thread::spawn(move || {
        let osk = HWND(osk_raw as *mut std::ffi::c_void);
        let hwnd = HWND(hwnd_raw as *mut std::ffi::c_void);
        let result = macro_engine::run(&model, stop.clone(), |a| unsafe { perform(osk, a) });
        if result {
            let _ = PostMessageW(Some(hwnd), WM_APP_DONE, WPARAM(0), LPARAM(0));
        } else if !stop.load(Ordering::Relaxed) {
            let message = Box::new(String::from("OSK input failed or a key could not be found"));
            let _ = PostMessageW(
                Some(hwnd),
                WM_APP_ERROR,
                WPARAM(0),
                LPARAM(Box::into_raw(message) as isize),
            );
        } else {
            let _ = PostMessageW(Some(hwnd), WM_APP_DONE, WPARAM(0), LPARAM(0));
        }
    });
}
unsafe fn stop_macro(ui: &mut Ui) {
    ui.stop_flag.store(true, Ordering::Relaxed);
    set_status(ui, "● Stopped");
}
unsafe fn perform(osk: HWND, a: &MacroAction) -> bool {
    match a {
        MacroAction::ModifierDown(k) => click(osk, k, 50).is_ok(),
        MacroAction::ModifierUp(k) => click(osk, k, 0).is_ok(),
        MacroAction::KeyDown(k) => click(osk, k, 50).is_ok(),
        MacroAction::KeyUp(k) => click(osk, k, 0).is_ok(),
        _ => true,
    }
}
unsafe fn click(parent: HWND, label: &str, hold: u64) -> Result<()> {
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
    SendInput(&[d], size_of::<INPUT>() as i32);
    thread::sleep(Duration::from_millis(hold));
    SendInput(&[u], size_of::<INPUT>() as i32);
    Ok(())
}
unsafe fn find_window(title: &str, class_name: &str) -> Option<HWND> {
    struct S {
        t: Vec<u16>,
        c: Vec<u16>,
        r: Option<HWND>,
    }
    unsafe extern "system" fn cb(h: HWND, p: LPARAM) -> windows::core::BOOL {
        let s = &mut *(p.0 as *mut S);
        if !IsWindowVisible(h).as_bool() {
            return true.into();
        }
        let mut c = [0u16; 256];
        let cl = GetClassNameW(h, &mut c) as usize;
        let mut t = [0u16; 512];
        let tl = GetWindowTextW(h, &mut t) as usize;
        if c[..cl] == s.c[..s.c.len() - 1]
            || t[..tl]
                .windows(s.t.len() - 1)
                .any(|w| w == &s.t[..s.t.len() - 1])
        {
            s.r = Some(h);
            return false.into();
        }
        true.into()
    }
    let mut s = S {
        t: wide(title),
        c: wide(class_name),
        r: None,
    };
    EnumWindows(Some(cb), LPARAM(&mut s as *mut _ as isize)).ok()?;
    s.r
}
unsafe fn set_enabled(hwnd: HWND, enabled: bool) {
    let state = if enabled { 0 } else { 1 };
    SendMessageW(hwnd, 0x000A, Some(WPARAM(state)), Some(LPARAM(0)));
}
unsafe fn set_status(ui: &mut Ui, text: &str) {
    let _ = SetWindowTextW(ui.status, PCWSTR(wide(text).as_ptr()));
    set_enabled(ui.start, text != "● Running");
    set_enabled(ui.stop, text == "● Running");
}
unsafe fn set_error(ui: &mut Ui, text: String) {
    ui.last_error = Some(text.clone());
    MessageBoxW(
        Some(ui.hwnd),
        PCWSTR(wide(&text).as_ptr()),
        PCWSTR(wide("OSK Macro Error").as_ptr()),
        MB_OK | MB_ICONERROR,
    );
    set_status(ui, "● Error");
}
unsafe fn register_hotkeys(h: HWND) {
    let _ = RegisterHotKey(h.0, 1, 0x0002, 0x50);
    let _ = RegisterHotKey(h.0, 2, 0x0002, 0x53);
}
unsafe fn unregister_hotkeys(h: HWND) {
    let _ = UnregisterHotKey(h.0, 1);
    let _ = UnregisterHotKey(h.0, 2);
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
