use std::fmt;

#[derive(Debug)]
pub enum SyncError {
    Io(String, std::io::Error),
    BadManifest(String),
    SourceUnavailable(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::Io(path, e) => write!(f, "ошибка ввода-вывода на '{path}': {e}"),
            SyncError::BadManifest(msg) => write!(f, "некорректный манифест: {msg}"),
            SyncError::SourceUnavailable(msg) => write!(f, "источник недоступен: {msg}"),
        }
    }
}

impl std::error::Error for SyncError {}
