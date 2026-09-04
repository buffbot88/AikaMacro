use crate::{
    accessibility,
    config::{self, AppConfig, InputMode, KeyBinding, SLOT_COUNT},
    editor,
    macro_engine::{self, MacroAction},
    osk::Session,
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
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, SetBkColor, SetBkMode,
            SetTextColor, TextOutW, HBRUSH, HDC, PAINTSTRUCT, TRANSPARENT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_SHIFT},
            WindowsAndMessaging::{
                AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DispatchMessageW,
                GetClientRect, GetMessageW, GetWindowLongPtrW, GetWindowTextW, LoadCursorW,
                MessageBoxW, MoveWindow, PostMessageW, PostQuitMessage, RegisterClassExW,
                SendMessageW, SetWindowLongPtrW, SetWindowTextW, ShowWindow, TranslateMessage,
                CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA, HMENU, IDC_ARROW, MB_ICONERROR, MB_OK,
                MINMAXINFO, MSG, SW_SHOW, WM_APP, WM_COMMAND, WM_CREATE, WM_DESTROY,
                WM_GETMINMAXINFO, WM_HOTKEY, WM_KEYDOWN, WM_NCCREATE, WM_PAINT, WM_SIZE,
                WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_APPWINDOW,
                WS_EX_CLIENTEDGE, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_THICKFRAME,
                WS_VISIBLE,
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
const WM_CTLCOLOR_EDIT: u32 = 0x0133;
const WM_CTLCOLOR_BTN: u32 = 0x0135;
const WM_CTLCOLOR_STATIC: u32 = 0x0138;

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterHotKey(hwnd: *mut c_void, id: i32, modifiers: u32, key: u32) -> i32;
    fn UnregisterHotKey(hwnd: *mut c_void, id: i32) -> i32;
    fn SetFocus(hwnd: *mut c_void) -> *mut c_void;
}

pub struct Ui {
    pub hwnd: HWND,
    osk: Session,
    model: Arc<Mutex<AppConfig>>,
    keys: [HWND; SLOT_COUNT],
    timers: [HWND; SLOT_COUNT],
    toggles: [HWND; SLOT_COUNT],
    slot_labels: [HWND; SLOT_COUNT],
    status: HWND,
    start: HWND,
    stop: HWND,
    mode: HWND,
    input_label: HWND,
    save: HWND,
    load: HWND,
    background_brush: HBRUSH,
    input_brush: HBRUSH,
    button_brush: HBRUSH,
    accent_brush: HBRUSH,
    status_color: COLORREF,
    stop_flag: Arc<AtomicBool>,
    capture: Option<usize>,
    editor: HWND,
    running: bool,
    last_error: Option<String>,
}

pub fn run(model: AppConfig) -> Result<()> {
    crate::logger::log("UI startup: starting fresh OSK session");
    let osk = Session::start()?;
    crate::logger::log("UI startup: OSK session ready");
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
        crate::logger::log("UI startup: window class registered");
        let state = Box::new(Ui {
            hwnd: HWND::default(),
            osk,
            model: Arc::new(Mutex::new(model)),
            keys: [HWND::default(); SLOT_COUNT],
            timers: [HWND::default(); SLOT_COUNT],
            toggles: [HWND::default(); SLOT_COUNT],
            slot_labels: [HWND::default(); SLOT_COUNT],
            status: HWND::default(),
            start: HWND::default(),
            stop: HWND::default(),
            mode: HWND::default(),
            input_label: HWND::default(),
            save: HWND::default(),
            load: HWND::default(),
            background_brush: HBRUSH::default(),
            input_brush: HBRUSH::default(),
            button_brush: HBRUSH::default(),
            accent_brush: HBRUSH::default(),
            status_color: COLORREF(0x009B9187),
            stop_flag: Arc::new(AtomicBool::new(false)),
            capture: None,
            editor: HWND::default(),
            running: false,
            last_error: None,
        });
        let window_style = WS_OVERLAPPED
            | WS_CAPTION
            | WS_SYSMENU
            | WS_MINIMIZEBOX
            | WS_THICKFRAME
            | WS_CLIPCHILDREN;
        let mut size = RECT {
            left: 0,
            top: 0,
            right: 960,
            bottom: 145,
        };
        AdjustWindowRectEx(&mut size, window_style, false, WS_EX_APPWINDOW)?;
        let state_ptr = Box::into_raw(state);
        let hwnd = match CreateWindowExW(
            WS_EX_APPWINDOW,
            PCWSTR(class.as_ptr()),
            PCWSTR(wide("OSK Macro").as_ptr()),
            window_style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            size.right - size.left,
            size.bottom - size.top,
            None,
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
        crate::logger::log(format!("UI startup: main window created hwnd={:?}", hwnd));
        let _ = ShowWindow(hwnd, SW_SHOW);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        crate::logger::log("UI shutdown: message loop ended");
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
            refresh(ui);
            layout(ui);
            register_hotkeys(hwnd);
            crate::logger::log("UI startup: controls and hotkeys initialized");
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let m = &mut *(lp.0 as *mut MINMAXINFO);
            let (min_w, min_h) = outer_size(820, 135);
            let (_, max_h) = outer_size(820, 145);
            m.ptMinTrackSize.x = min_w;
            m.ptMinTrackSize.y = min_h;
            m.ptMaxTrackSize.y = max_h;
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
        0x002B => {
            draw_item(lp);
            LRESULT(1)
        }
        WM_CTLCOLOR_EDIT | WM_CTLCOLOR_BTN | WM_CTLCOLOR_STATIC => color_control(ui, msg, wp, lp),
        WM_COMMAND => {
            command(ui, wp);
            LRESULT(0)
        }
        WM_HOTKEY => {
            crate::logger::log(format!("UI hotkey received id={}", wp.0));
            if wp.0 == 1 {
                start_macro(ui)
            } else if wp.0 == 2 {
                stop_macro(ui)
            };
            LRESULT(0)
        }
        editor::WM_EDITOR_RESULT => {
            let result = Box::from_raw(lp.0 as *mut (usize, crate::config::MacroSlot));
            if result.0 < SLOT_COUNT {
                ui.model.lock().unwrap().slots[result.0] = result.1;
                refresh(ui);
            }
            LRESULT(0)
        }
        editor::WM_EDITOR_CLOSED => {
            ui.editor = HWND::default();
            LRESULT(0)
        }
        WM_APP_DONE => {
            ui.running = false;
            set_status(ui, "● Stopped");
            LRESULT(0)
        }
        WM_APP_ERROR => {
            ui.running = false;
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
            crate::logger::log("UI shutdown: main window destroyed");
            unregister_hotkeys(hwnd);
            ui.stop_flag.store(true, Ordering::Relaxed);
            if !ui.editor.is_invalid() {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(ui.editor);
            }
            ui.osk.shutdown();
            if !ui.background_brush.is_invalid() {
                let _ = DeleteObject(ui.background_brush.into());
            }
            if !ui.input_brush.is_invalid() {
                let _ = DeleteObject(ui.input_brush.into());
            }
            if !ui.button_brush.is_invalid() {
                let _ = DeleteObject(ui.button_brush.into());
            }
            if !ui.accent_brush.is_invalid() {
                let _ = DeleteObject(ui.accent_brush.into());
            }
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
    ui.save = button(p, "Save", ID_SAVE);
    ui.load = button(p, "Load", ID_LOAD);
    ui.input_label = label(p, "Input:");
    ui.mode = combo(p, ID_MODE);
    ui.status = label(p, "● Stopped");
    ui.background_brush = CreateSolidBrush(COLORREF(0x00201B17));
    ui.input_brush = CreateSolidBrush(COLORREF(0x00312B25));
    ui.button_brush = CreateSolidBrush(COLORREF(0x002C343C));
    ui.accent_brush = CreateSolidBrush(COLORREF(0x00E89724));
    for i in 0..8 {
        ui.slot_labels[i] = label(p, &format!("{}", i + 1));
        ui.toggles[i] = toggle(p, ID_TOGGLE_BASE + i);
        ui.keys[i] = edit(p, "Set", ID_SLOT_BASE + i);
        ui.timers[i] = edit(p, "1000", ID_TIMER_BASE + i);
    }
}
unsafe fn button(p: HWND, t: &str, id: usize) -> HWND {
    CreateWindowExW(
        Default::default(),
        w!("BUTTON"),
        PCWSTR(wide(t).as_ptr()),
        WS_CHILD | WS_VISIBLE | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(0x000B),
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
unsafe fn toggle(p: HWND, id: usize) -> HWND {
    CreateWindowExW(
        Default::default(),
        w!("BUTTON"),
        PCWSTR(wide("●").as_ptr()),
        WS_CHILD | WS_VISIBLE | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(0x0003),
        0,
        0,
        22,
        26,
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
        22,
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
        22,
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
unsafe fn outer_size(client_width: i32, client_height: i32) -> (i32, i32) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: client_width,
        bottom: client_height,
    };
    let style =
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_THICKFRAME | WS_CLIPCHILDREN;
    let _ = windows::Win32::UI::WindowsAndMessaging::AdjustWindowRectEx(
        &mut rect,
        style,
        false,
        WS_EX_APPWINDOW,
    );
    (rect.right - rect.left, rect.bottom - rect.top)
}
unsafe fn layout(ui: &mut Ui) {
    let mut r = RECT::default();
    GetClientRect(ui.hwnd, &mut r).ok();
    let w = r.right.max(1);
    let h = r.bottom.max(1);
    let header_h = 34;
    let footer_h = 34;
    let footer_y = (h - footer_h).max(header_h + 32);
    let strip_y = header_h + 4;
    let strip_h = (footer_y - strip_y - 4).max(28);
    let row_y = strip_y + (strip_h - 28).max(0) / 2;

    let _ = MoveWindow(ui.input_label, w - 420, 7, 45, 20, true);
    let _ = MoveWindow(ui.mode, w - 370, 4, 130, 26, true);
    let _ = MoveWindow(ui.status, w - 225, 7, 130, 22, true);
    for i in 0..SLOT_COUNT {
        let x = 8 + (w - 16) * i as i32 / SLOT_COUNT as i32;
        let next = 8 + (w - 16) * (i as i32 + 1) / SLOT_COUNT as i32;
        let slot_width = next - x;
        let _ = MoveWindow(ui.slot_labels[i], x, row_y + 5, 16, 18, true);
        let _ = MoveWindow(ui.toggles[i], x + 17, row_y + 1, 22, 26, true);
        let _ = MoveWindow(
            ui.keys[i],
            x + 41,
            row_y,
            (slot_width - 94).max(34),
            28,
            true,
        );
        let _ = MoveWindow(ui.timers[i], next - 49, row_y, 45, 28, true);
    }
    let button_y = footer_y + 2;
    let _ = MoveWindow(ui.start, 8, button_y, 105, 30, true);
    let _ = MoveWindow(ui.stop, 120, button_y, 90, 30, true);
    let _ = MoveWindow(ui.save, 217, button_y, 80, 30, true);
    let _ = MoveWindow(ui.load, 304, button_y, 80, 30, true);
}
#[allow(dead_code)]
unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let footer_y = (client.bottom - 34).max(34);
    let base = CreateSolidBrush(COLORREF(0x00201B17));
    let header = CreateSolidBrush(COLORREF(0x001D2228));
    let slots = CreateSolidBrush(COLORREF(0x0020262C));
    let _ = FillRect(hdc, &client, base);
    let header_rect = RECT {
        left: 0,
        top: 0,
        right: client.right,
        bottom: 34,
    };
    let strip_rect = RECT {
        left: 0,
        top: 34,
        right: client.right,
        bottom: footer_y,
    };
    let footer_rect = RECT {
        left: 0,
        top: footer_y,
        right: client.right,
        bottom: client.bottom,
    };
    let _ = FillRect(hdc, &header_rect, header);
    let _ = FillRect(hdc, &strip_rect, slots);
    let _ = FillRect(hdc, &footer_rect, header);
    let separator_brush = CreateSolidBrush(COLORREF(0x00343C45));
    for i in 1..8 {
        let x = client.right * i / 8;
        let separator = RECT {
            left: x,
            top: 38,
            right: x + 1,
            bottom: footer_y - 4,
        };
        let _ = FillRect(hdc, &separator, separator_brush);
    }
    let _ = DeleteObject(separator_brush.into());
    let _ = DeleteObject(base.into());
    let _ = DeleteObject(header.into());
    let _ = DeleteObject(slots.into());
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, COLORREF(0x00F1F4F7));
    let title = wide("OSK Macro");
    let subtitle = wide("Compact Windows keyboard macro");
    let hints = wide("Ctrl+P Start  •  Ctrl+S Stop");
    let _ = TextOutW(hdc, 12, 4, &title[..title.len() - 1]);
    SetTextColor(hdc, COLORREF(0x00A7B1BB));
    let _ = TextOutW(hdc, 12, 21, &subtitle[..subtitle.len() - 1]);
    let _ = TextOutW(hdc, 530, footer_y + 10, &hints[..hints.len() - 1]);
    let _ = EndPaint(hwnd, &ps);
}
#[repr(C)]
struct DrawItem {
    ctl_type: u32,
    ctl_id: u32,
    item_id: u32,
    item_action: u32,
    item_state: u32,
    hwnd_item: HWND,
    hdc: HDC,
    rect: RECT,
    item_data: usize,
}

unsafe fn draw_item(lp: LPARAM) {
    if lp.0 == 0 {
        return;
    }
    let item = &*(lp.0 as *const DrawItem);
    let id = item.ctl_id as usize;
    let (background, foreground, text) = match id {
        ID_START => (COLORREF(0x00E89724), COLORREF(0x00FFFFFF), "Start"),
        ID_STOP => (COLORREF(0x00343C45), COLORREF(0x00F1F4F7), "Stop"),
        ID_SAVE => (COLORREF(0x00343C45), COLORREF(0x00F1F4F7), "Save"),
        ID_LOAD => (COLORREF(0x00343C45), COLORREF(0x00F1F4F7), "Load"),
        _ => return,
    };
    let disabled = item.item_state & 0x0004 != 0;
    let pressed = item.item_state & 0x0001 != 0;
    let color = if disabled {
        COLORREF(0x0023292F)
    } else if pressed {
        COLORREF(0x0036A8F5)
    } else {
        background
    };
    let brush = CreateSolidBrush(color);
    let _ = FillRect(item.hdc, &item.rect, brush);
    let _ = DeleteObject(brush.into());
    SetBkMode(item.hdc, TRANSPARENT);
    SetTextColor(
        item.hdc,
        if disabled {
            COLORREF(0x00758089)
        } else {
            foreground
        },
    );
    let value = wide(text);
    let width = item.rect.right - item.rect.left;
    let text_width = (text.chars().count() as i32 * 8).min(width - 8);
    let x = item.rect.left + (width - text_width).max(0) / 2;
    let y = item.rect.top + ((item.rect.bottom - item.rect.top - 16) / 2).max(0);
    let _ = TextOutW(item.hdc, x, y, &value[..value.len() - 1]);
}

unsafe fn color_control(ui: &Ui, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let hdc = HDC(wp.0 as *mut c_void);
    let child = HWND(lp.0 as *mut c_void);
    if message == WM_CTLCOLOR_STATIC && child == ui.status {
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, ui.status_color);
        return LRESULT(ui.background_brush.0 as isize);
    }
    if message == WM_CTLCOLOR_STATIC {
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(0x00F1F4F7));
        return LRESULT(ui.background_brush.0 as isize);
    }
    if message == WM_CTLCOLOR_EDIT {
        SetBkMode(hdc, windows::Win32::Graphics::Gdi::BACKGROUND_MODE(2));
        SetBkColor(hdc, COLORREF(0x00312B25));
        SetTextColor(hdc, COLORREF(0x00F1F4F7));
        return LRESULT(ui.input_brush.0 as isize);
    }
    SetBkMode(hdc, windows::Win32::Graphics::Gdi::BACKGROUND_MODE(2));
    SetBkColor(
        hdc,
        if child == ui.start {
            COLORREF(0x00E89724)
        } else {
            COLORREF(0x002C343C)
        },
    );
    SetTextColor(hdc, COLORREF(0x00F1F4F7));
    if child == ui.start {
        LRESULT(ui.accent_brush.0 as isize)
    } else {
        LRESULT(ui.button_brush.0 as isize)
    }
}
unsafe fn open_editor(ui: &mut Ui, index: usize) {
    if index >= SLOT_COUNT || !ui.editor.is_invalid() {
        return;
    }
    sync(ui);
    let slot = ui.model.lock().unwrap().slots[index].clone();
    match editor::open(ui.hwnd, index, slot) {
        Ok(hwnd) => ui.editor = hwnd,
        Err(error) => set_error(ui, error.to_string()),
    }
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
            if (wp.0 >> 16) as u16 != 0x0100 {
                return;
            }
            let i = id - ID_SLOT_BASE;
            ui.capture = Some(i);
            let _ = SetWindowTextW(ui.keys[i], PCWSTR(wide("Press...").as_ptr()));
            SetFocus(ui.hwnd.0);
        }
        id if (ID_TIMER_BASE..ID_TIMER_BASE + 8).contains(&id) => {
            if (wp.0 >> 16) as u16 == 0x0100 {
                open_editor(ui, id - ID_TIMER_BASE);
            }
        }
        id if (ID_TOGGLE_BASE..ID_TOGGLE_BASE + 8).contains(&id) => {
            if (wp.0 >> 16) as u16 == 0 {
                sync(ui);
            }
        }
        ID_MODE => {
            if (wp.0 >> 16) as u16 == 1 {
                sync(ui);
            }
        }
        _ => {}
    }
}
unsafe fn sync(ui: &mut Ui) {
    let mut m = ui.model.lock().unwrap();
    let mode = SendMessageW(ui.mode, 0x0147, Some(WPARAM(0)), Some(LPARAM(0))).0;
    m.input_mode = if mode == 1 {
        InputMode::ScanCode
    } else {
        InputMode::VirtualKey
    };
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
        m.slots[i].key =
            if label.is_empty() || label == "Set" || label == "Press" || label == "Press..." {
                None
            } else {
                Some(parse_binding(&label))
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
    SendMessageW(
        ui.mode,
        0x014E,
        Some(WPARAM(match m.input_mode {
            InputMode::VirtualKey => 0,
            InputMode::ScanCode => 1,
        })),
        Some(LPARAM(0)),
    );
    for i in 0..8 {
        let _ = SetWindowTextW(
            ui.keys[i],
            PCWSTR(
                wide(
                    &m.slots[i]
                        .key
                        .as_ref()
                        .map(binding_display)
                        .unwrap_or_else(|| "Set".to_string()),
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
    if value == 0x1B {
        let _ = SetWindowTextW(ui.keys[i], PCWSTR(wide("Set").as_ptr()));
        let mut m = ui.model.lock().unwrap();
        m.slots[i].key = None;
        return;
    }
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
    let binding = KeyBinding { label, ctrl, shift };
    let display = binding_display(&binding);
    let _ = SetWindowTextW(ui.keys[i], PCWSTR(wide(&display).as_ptr()));
    let mut m = ui.model.lock().unwrap();
    m.slots[i].key = Some(binding);
}
unsafe fn start_macro(ui: &mut Ui) {
    crate::logger::log("macro start requested");
    if ui.running {
        return;
    }
    sync(ui);
    let model = ui.model.lock().unwrap().clone();
    if !model.slots.iter().any(|s| s.enabled && s.key.is_some()) {
        set_error(ui, "Configure at least one enabled slot".into());
        return;
    }
    let osk = ui.osk.hwnd;
    crate::logger::log(format!(
        "macro starting with {} enabled slot(s)",
        model
            .slots
            .iter()
            .filter(|slot| slot.enabled && slot.key.is_some())
            .count()
    ));
    ui.stop_flag.store(false, Ordering::Relaxed);
    ui.running = true;
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
        crate::logger::log("macro worker started");
        let mut failure = None;
        let result = macro_engine::run(&model, stop.clone(), |a| match perform(osk, a) {
            Ok(()) => true,
            Err(error) => {
                failure = Some(format!("{error:#}"));
                false
            }
        });
        if result {
            crate::logger::log("macro worker completed successfully");
            let _ = PostMessageW(Some(hwnd), WM_APP_DONE, WPARAM(0), LPARAM(0));
        } else if !stop.load(Ordering::Relaxed) {
            let message = failure
                .unwrap_or_else(|| String::from("OSK input failed or a key could not be found"));
            crate::logger::log(format!("macro worker failed: {message}"));
            let message = Box::new(message);
            let _ = PostMessageW(
                Some(hwnd),
                WM_APP_ERROR,
                WPARAM(0),
                LPARAM(Box::into_raw(message) as isize),
            );
        } else {
            crate::logger::log("macro worker stopped by request");
            let _ = PostMessageW(Some(hwnd), WM_APP_DONE, WPARAM(0), LPARAM(0));
        }
    });
}
unsafe fn stop_macro(ui: &mut Ui) {
    crate::logger::log("macro stop requested");
    ui.stop_flag.store(true, Ordering::Relaxed);
    ui.running = false;
    set_status(ui, "● Stopped");
}
fn perform(osk: HWND, a: &MacroAction) -> Result<()> {
    match a {
        MacroAction::ModifierDown(k) => accessibility::invoke_control(osk, k),
        MacroAction::ModifierUp(k) => accessibility::invoke_control(osk, k),
        MacroAction::KeyDown(k) => accessibility::invoke_control(osk, k),
        // UI Automation Invoke is an atomic activation; the configured Hold action
        // already provides the delay before this KeyUp action is reached.
        MacroAction::KeyUp(_) | MacroAction::Hold(_) | MacroAction::Delay(_) => Ok(()),
    }
}
unsafe fn set_enabled(hwnd: HWND, enabled: bool) {
    let state = if enabled { 0 } else { 1 };
    SendMessageW(hwnd, 0x000A, Some(WPARAM(state)), Some(LPARAM(0)));
}
unsafe fn set_status(ui: &mut Ui, text: &str) {
    ui.status_color = match text {
        "● Running" => COLORREF(0x008EC748),
        "● Error" => COLORREF(0x006060E0),
        _ => COLORREF(0x009B9187),
    };
    let _ = SetWindowTextW(ui.status, PCWSTR(wide(text).as_ptr()));
    set_enabled(ui.start, text != "● Running");
    set_enabled(ui.stop, text == "● Running");
}
unsafe fn set_error(ui: &mut Ui, text: String) {
    crate::logger::log(format!("UI error: {text}"));
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
    let start = RegisterHotKey(h.0, 1, 0x0002, 0x50);
    let stop = RegisterHotKey(h.0, 2, 0x0002, 0x53);
    crate::logger::log(format!(
        "hotkey registration: Ctrl+P={} Ctrl+S={}",
        start != 0,
        stop != 0
    ));
}
unsafe fn unregister_hotkeys(h: HWND) {
    let _ = UnregisterHotKey(h.0, 1);
    let _ = UnregisterHotKey(h.0, 2);
}
fn binding_display(binding: &KeyBinding) -> String {
    match (binding.ctrl, binding.shift) {
        (true, true) => format!("Ctrl+Shift+{}", binding.label),
        (true, false) => format!("Ctrl+{}", binding.label),
        (false, true) => format!("Shift+{}", binding.label),
        (false, false) => binding.label.clone(),
    }
}

fn parse_binding(value: &str) -> KeyBinding {
    let mut ctrl = false;
    let mut shift = false;
    let mut parts = value.split('+').collect::<Vec<_>>();
    while let Some(prefix) = parts.first().map(|part| part.trim().to_ascii_lowercase()) {
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

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
