use crate::logger;
use anyhow::{Context, Result};
use std::{
    ffi::c_void,
    mem::size_of,
    thread,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextW, IsWindowVisible, PostMessageW, WM_CLOSE,
    },
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const SEE_MASK_NOCLOSEPROCESS: u32 = 0x0000_0040;
const SW_SHOWNORMAL: i32 = 1;
const PROCESS_TERMINATE: u32 = 0x0001;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const SYNCHRONIZE: u32 = 0x0010_0000;
const WAIT_TIMEOUT: u32 = 258;

#[repr(C)]
struct ShellExecuteInfoW {
    cb_size: u32,
    f_mask: u32,
    hwnd: *mut c_void,
    lp_verb: *const u16,
    lp_file: *const u16,
    lp_parameters: *const u16,
    lp_directory: *const u16,
    n_show: i32,
    h_inst_app: *mut c_void,
    lp_id_list: *mut c_void,
    lp_class: *const u16,
    h_key_class: *mut c_void,
    dw_hot_key: u32,
    h_icon_or_monitor: *mut c_void,
    h_process: *mut c_void,
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteExW(info: *mut ShellExecuteInfoW) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn GetLastError() -> u32;
    fn GetProcessId(handle: *mut c_void) -> u32;
    fn OpenProcess(access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
    fn TerminateProcess(handle: *mut c_void, exit_code: u32) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
}

pub struct Session {
    pub hwnd: HWND,
    process: *mut c_void,
    closed: bool,
}

impl Session {
    pub fn start() -> Result<Self> {
        logger::log("OSK startup: closing existing OSK windows");
        close_existing()?;
        logger::log("OSK startup: existing OSK windows closed");

        let file = wide("osk.exe");
        let verb = wide("runas");
        let mut info = ShellExecuteInfoW {
            cb_size: size_of::<ShellExecuteInfoW>() as u32,
            f_mask: SEE_MASK_NOCLOSEPROCESS,
            hwnd: std::ptr::null_mut(),
            lp_verb: verb.as_ptr(),
            lp_file: file.as_ptr(),
            lp_parameters: std::ptr::null(),
            lp_directory: std::ptr::null(),
            n_show: SW_SHOWNORMAL,
            h_inst_app: std::ptr::null_mut(),
            lp_id_list: std::ptr::null_mut(),
            lp_class: std::ptr::null(),
            h_key_class: std::ptr::null_mut(),
            dw_hot_key: 0,
            h_icon_or_monitor: std::ptr::null_mut(),
            h_process: std::ptr::null_mut(),
        };
        let launched = unsafe { ShellExecuteExW(&mut info) };
        if launched == 0 {
            let error = std::io::Error::last_os_error();
            logger::log(format!("OSK startup: elevated launch failed: {error}"));
            return Err(anyhow::anyhow!(
                "could not start elevated Windows On-Screen Keyboard (osk.exe): {error}"
            ));
        }
        if info.h_process.is_null() {
            anyhow::bail!("Windows started OSK but did not return a process handle");
        }
        logger::log(format!(
            "OSK startup: elevated launch accepted pid={}",
            unsafe { GetProcessId(info.h_process) }
        ));

        let deadline = Instant::now() + STARTUP_TIMEOUT;
        loop {
            if let Some(hwnd) = find_windows().into_iter().next() {
                logger::log(format!("OSK startup: ready hwnd={:?}", hwnd));
                return Ok(Self {
                    hwnd,
                    process: info.h_process,
                    closed: false,
                });
            }
            if Instant::now() >= deadline {
                terminate_owned_process(info.h_process, "startup timeout");
                unsafe {
                    let _ = CloseHandle(info.h_process);
                }
                logger::log("OSK startup: timed out waiting for the OSK window");
                anyhow::bail!("On-Screen Keyboard did not create a window within 10 seconds")
            }
            thread::sleep(POLL_INTERVAL);
        }
    }
}

impl Session {
    pub fn shutdown(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        logger::log(format!("OSK shutdown: closing hwnd={:?}", self.hwnd));
        unsafe {
            if PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).is_err() {
                logger::log(format!(
                    "OSK shutdown: WM_CLOSE failed with Windows error {}",
                    GetLastError()
                ));
            }
        }
        let deadline = Instant::now() + CLOSE_TIMEOUT;
        while window_exists(self.hwnd) && Instant::now() < deadline {
            thread::sleep(POLL_INTERVAL);
        }
        if window_exists(self.hwnd) || process_running(self.process) {
            terminate_owned_process(self.process, "shutdown");
        }
        unsafe {
            let _ = CloseHandle(self.process);
        }
        logger::log("OSK shutdown: complete");
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn terminate_owned_process(process: *mut c_void, reason: &str) {
    let pid = unsafe { GetProcessId(process) };
    let mut termination_handle = process;
    let mut opened = false;
    if pid != 0 {
        let candidate = unsafe {
            OpenProcess(
                PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                0,
                pid,
            )
        };
        if !candidate.is_null() {
            termination_handle = candidate;
            opened = true;
        }
    }
    let terminated = unsafe { TerminateProcess(termination_handle, 1) != 0 };
    if terminated {
        logger::log(format!("OSK process pid={pid} terminated ({reason})"));
        unsafe {
            let _ = WaitForSingleObject(termination_handle, 2_000);
        }
    } else {
        logger::log(format!(
            "OSK process pid={pid} termination failed ({reason}); Windows error {}",
            unsafe { GetLastError() }
        ));
    }
    if opened {
        unsafe {
            let _ = CloseHandle(termination_handle);
        }
    }
}

fn process_running(process: *mut c_void) -> bool {
    unsafe { WaitForSingleObject(process, 0) == WAIT_TIMEOUT }
}

fn close_existing() -> Result<()> {
    let existing = find_windows();
    logger::log(format!(
        "OSK startup: found {} existing window(s)",
        existing.len()
    ));
    close_windows()?;
    let deadline = Instant::now() + CLOSE_TIMEOUT;
    while !find_windows().is_empty() {
        if Instant::now() >= deadline {
            logger::log("OSK startup: existing OSK close timed out");
            anyhow::bail!("an existing On-Screen Keyboard window did not close within 5 seconds")
        }
        thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}

fn close_windows() -> Result<()> {
    for hwnd in find_windows() {
        logger::log(format!("OSK startup: requesting hwnd={:?} to close", hwnd));
        unsafe {
            PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0))
                .context("could not request existing OSK window to close")?;
        }
    }
    Ok(())
}

fn window_exists(target: HWND) -> bool {
    find_windows().into_iter().any(|hwnd| hwnd == target)
}

fn find_windows() -> Vec<HWND> {
    struct Search {
        title: Vec<u16>,
        class_name: Vec<u16>,
        results: Vec<HWND>,
    }
    unsafe extern "system" fn callback(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
        let search = &mut *(data.0 as *mut Search);
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }
        let mut class_buffer = [0u16; 256];
        let class_len = GetClassNameW(hwnd, &mut class_buffer) as usize;
        let mut title_buffer = [0u16; 512];
        let title_len = GetWindowTextW(hwnd, &mut title_buffer) as usize;
        let class_match =
            class_buffer[..class_len] == search.class_name[..search.class_name.len() - 1];
        let title = String::from_utf16_lossy(&title_buffer[..title_len]);
        let wanted_title = String::from_utf16_lossy(&search.title[..search.title.len() - 1]);
        let title_match = title
            .to_ascii_lowercase()
            .contains(&wanted_title.to_ascii_lowercase());
        if class_match || title_match {
            search.results.push(hwnd);
        }
        true.into()
    }

    let mut search = Search {
        title: wide("On-Screen Keyboard"),
        class_name: wide("OSKMainClass"),
        results: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(callback), LPARAM(&mut search as *mut Search as isize));
    }
    search.results
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
