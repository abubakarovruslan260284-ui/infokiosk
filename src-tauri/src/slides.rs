use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct SlideDto {
    pub name: String,
    /// "image" | "video" — фронтенду не нужно самому парсить расширение.
    pub kind: String,
    /// Абсолютный путь на диске. Фронтенд превращает его в отображаемый
    /// URL через convertFileSrc() из @tauri-apps/api/core — это отдаёт
    /// файл нативным потоком (asset-протокол), БЕЗ base64 и БЕЗ накладных
    /// расходов IPC-канала. Для видео (единицы-десятки МБ) это критично:
    /// прогонять такие файлы через base64/JSON на слабом ПК означало бы
    /// как раз воссоздать те тормоза, от которых мы уходим.
    pub path: String,
}

/// Читает все медиафайлы активного кэша и отдаёт метаданные (без самого
/// содержимого). Сортировка — по имени файла, поэтому порядок показа
/// задаётся префиксами вроде `01_`, `02_...` в названиях файлов на
/// стороне «Издателя».
pub fn read_slides(active_dir: &Path) -> Vec<SlideDto> {
    let mut entries: Vec<_> = match std::fs::read_dir(active_dir) {
        Ok(r) => r.filter_map(|e| e.ok()).collect(),
        Err(_) => return vec![],
    };
    entries.sort_by_key(|e| e.file_name());

    let mut out = Vec::new();
    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name == ".local-manifest.json" || name == "manifest.json" {
            continue;
        }
        let Some(kind) = kind_for(&name) else { continue };
        out.push(SlideDto {
            name,
            kind: kind.to_string(),
            path: path.to_string_lossy().to_string(),
        });
    }
    out
}

fn kind_for(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    if [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"].iter().any(|e| lower.ends_with(e)) {
        Some("image")
    } else if [".mp4", ".webm"].iter().any(|e| lower.ends_with(e)) {
        Some("video")
    } else {
        None
    }
}
