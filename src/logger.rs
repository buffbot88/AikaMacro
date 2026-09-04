use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct Logger {
    enabled: Arc<AtomicBool>,
    path: PathBuf,
    file: Arc<Mutex<Option<std::fs::File>>>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

pub fn init(enabled: bool, path: PathBuf) -> Logger {
    let logger = Logger {
        enabled: Arc::new(AtomicBool::new(enabled)),
        path,
        file: Arc::new(Mutex::new(None)),
    };
    let _ = LOGGER.set(logger.clone());
    logger
}

pub fn global() -> Option<&'static Logger> {
    LOGGER.get()
}

impl Logger {
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn log(&self, message: impl AsRef<str>) {
        if !self.enabled() {
            return;
        }
        let Ok(mut file) = self.file.lock() else {
            return;
        };
        if file.is_none() {
            *file = open_file(&self.path);
        }
        let Some(file) = file.as_mut() else {
            return;
        };
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let _ = writeln!(file, "[{timestamp}] {}", message.as_ref());
        let _ = file.flush();
    }
}

fn open_file(path: &Path) -> Option<std::fs::File> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

pub fn log(message: impl AsRef<str>) {
    if let Some(logger) = global() {
        logger.log(message);
    }
}
