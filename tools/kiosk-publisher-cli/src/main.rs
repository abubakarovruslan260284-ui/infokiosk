//! kiosk_publish — минимальный «Издатель» из командной строки.
//!
//! Кладёте картинки/GIF/видео в папку → запускаете эту утилиту, указав
//! путь к папке → она пересчитывает manifest.json (версия = таймстамп,
//! хэши всех файлов). Как только манифест обновился на диске (в общей
//! сетевой папке), все подключённые к ней киоски заберут изменения при
//! следующем опросе — без перезапуска и без ручного копирования.
//!
//! Запуск:  kiosk_publish <папка-с-контентом> [версия]
//! Если версия не указана — берётся текущее время (UTC, секунды).

use kiosk_sync::Manifest;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = match args.next() {
        Some(d) => PathBuf::from(d),
        None => {
            eprintln!("Использование: kiosk_publish <папка-с-контентом> [версия]");
            std::process::exit(1);
        }
    };
    let version = args.next().unwrap_or_else(default_version);

    if !dir.is_dir() {
        eprintln!("Ошибка: '{}' — не папка или не существует", dir.display());
        std::process::exit(1);
    }

    match Manifest::from_directory(&dir, version.clone()) {
        Ok(manifest) => {
            let manifest_path = dir.join("manifest.json");
            if let Err(e) = manifest.save(&manifest_path) {
                eprintln!("Не удалось сохранить манифест: {e}");
                std::process::exit(1);
            }
            println!("Опубликовано: версия '{}', файлов: {}", version, manifest.files.len());
            for f in &manifest.files {
                println!("  - {}  ({} байт, sha256 {}…)", f.name, f.size, &f.sha256[..12]);
            }
            println!("\nМанифест записан: {}", manifest_path.display());
            println!("Киоски, следящие за этой папкой, заберут изменения при следующем опросе.");

            warn_about_skipped_files(&dir, &manifest);
        }
        Err(e) => {
            eprintln!("Ошибка при построении манифеста: {e}");
            std::process::exit(1);
        }
    }
}

/// Инфокиоск показывает только то, что умеет декодировать браузерный
/// движок (PNG/JPEG/GIF/WEBP/BMP/MP4/WEBM). Формат HEIC/HEIF (по
/// умолчанию используется камерой iPhone) веб-браузеры не поддерживают
/// вообще — такой файл не покажется молча. Поэтому явно предупреждаем,
/// а не оставляем это в тишине: до этого именно так терялись фото.
fn warn_about_skipped_files(dir: &PathBuf, manifest: &Manifest) {
    let published: std::collections::HashSet<&str> =
        manifest.files.iter().map(|f| f.name.as_str()).collect();

    let mut heic_files = Vec::new();
    let mut other_skipped = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if name == "manifest.json" || published.contains(name.as_str()) {
                continue;
            }
            let lower = name.to_lowercase();
            if lower.ends_with(".heic") || lower.ends_with(".heif") {
                heic_files.push(name);
            } else if lower.ends_with(".mov") || lower.ends_with(".tiff") || lower.ends_with(".tif")
                || lower.ends_with(".raw") || lower.ends_with(".psd") || lower.ends_with(".svg")
            {
                other_skipped.push(name);
            }
        }
    }

    if !heic_files.is_empty() {
        println!("\n⚠ Не опубликовано ({} файлов) — формат HEIC/HEIF с iPhone браузер не умеет показывать:", heic_files.len());
        for f in &heic_files {
            println!("    - {f}");
        }
        println!("  Как исправить: на iPhone — Настройки → Камера → Форматы → «Наиболее совместимые»");
        println!("  (тогда новые фото будут сразу в JPEG), либо откройте фото в приложении «Фото» на iPhone");
        println!("  и через «Экспортировать» сохраните как JPEG перед копированием в эту папку.");
    }
    if !other_skipped.is_empty() {
        println!("\n⚠ Не опубликовано ({} файлов) — неподдерживаемый формат:", other_skipped.len());
        for f in &other_skipped {
            println!("    - {f}");
        }
        println!("  Поддерживаются: PNG, JPG, GIF, WEBP, BMP (фото/картинки), MP4, WEBM (видео).");
    }
}

fn default_version() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    format!("auto-{secs}")
}
