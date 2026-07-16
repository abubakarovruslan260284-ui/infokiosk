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
        }
        Err(e) => {
            eprintln!("Ошибка при построении манифеста: {e}");
            std::process::exit(1);
        }
    }
}

fn default_version() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    format!("auto-{secs}")
}
