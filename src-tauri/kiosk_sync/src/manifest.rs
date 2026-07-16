//! Модель манифеста контента.
//!
//! Манифест — единственный файл, за которым следит киоск. Он маленький
//! (килобайты), поэтому его можно вычитывать часто и дёшево, не трогая
//! сами медиафайлы, пока ничего не изменилось.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;

use crate::error::SyncError;

/// Одна запись о файле в манифесте.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    /// Имя файла (относительный путь внутри папки контента).
    pub name: String,
    /// SHA-256 содержимого файла, hex-строка.
    pub sha256: String,
    /// Размер в байтах — используется для быстрой предварительной
    /// проверки и для отображения прогресса докачки.
    pub size: u64,
    /// Порядок показа на киоске (меньше — раньше). Необязателен;
    /// если не задан, используется сортировка по имени.
    #[serde(default)]
    pub order: Option<i64>,
}

/// Манифест целиком: версия набора + список файлов.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Manifest {
    /// Версия набора контента. Меняется при любом изменении файлов.
    /// Может быть счётчиком, таймстампом или хэшем — киоску всё равно,
    /// он лишь сравнивает строки на равенство.
    pub version: String,
    pub files: Vec<FileEntry>,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Manifest, SyncError> {
        let data = fs::read_to_string(path).map_err(|e| SyncError::Io(path.display().to_string(), e))?;
        serde_json::from_str(&data).map_err(|e| SyncError::BadManifest(e.to_string()))
    }

    pub fn save(&self, path: &Path) -> Result<(), SyncError> {
        let data = serde_json::to_string_pretty(self).map_err(|e| SyncError::BadManifest(e.to_string()))?;
        fs::write(path, data).map_err(|e| SyncError::Io(path.display().to_string(), e))
    }

    /// Индекс файлов манифеста по имени — удобно для diff'а.
    pub fn by_name(&self) -> HashMap<&str, &FileEntry> {
        self.files.iter().map(|f| (f.name.as_str(), f)).collect()
    }

    /// Строит манифест по содержимому директории (сторона издателя).
    /// Не рекурсивный — контент слайдера плоский, это осознанное упрощение.
    pub fn from_directory(dir: &Path, version: impl Into<String>) -> Result<Manifest, SyncError> {
        let mut files = Vec::new();
        let read_dir = fs::read_dir(dir).map_err(|e| SyncError::Io(dir.display().to_string(), e))?;
        for entry in read_dir {
            let entry = entry.map_err(|e| SyncError::Io(dir.display().to_string(), e))?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            if name == "manifest.json" {
                continue;
            }
            if !is_media_file(&name) {
                continue;
            }
            let size = entry.metadata().map_err(|e| SyncError::Io(path.display().to_string(), e))?.len();
            let sha256 = hash_file(&path)?;
            files.push(FileEntry { name, sha256, size, order: None });
        }
        files.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Manifest { version: version.into(), files })
    }
}

fn is_media_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".mp4", ".webm"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// SHA-256 файла в виде hex-строки.
pub fn hash_file(path: &Path) -> Result<String, SyncError> {
    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path).map_err(|e| SyncError::Io(path.display().to_string(), e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| SyncError::Io(path.display().to_string(), e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
