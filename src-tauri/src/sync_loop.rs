use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::settings::KioskSettings;

/// Пишем в файл, а не в stderr: в собранном .exe (windows_subsystem =
/// "windows") консоли нет, и eprintln! просто пропадает в никуда — тогда
/// диагностировать проблему на реальном киоске становится невозможно.
/// Этот файл можно открыть блокнотом и увидеть, что реально происходило.
fn log_line(log_path: &PathBuf, msg: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[{now}] {msg}\n");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = f.write_all(line.as_bytes());
    }
    #[cfg(debug_assertions)]
    eprintln!("{msg}");
}

/// Держим лог небольшим — обрезаем, если разросся за ~500 КБ, чтобы не
/// копить его годами на диске киоска.
fn trim_log_if_large(log_path: &PathBuf) {
    if let Ok(meta) = std::fs::metadata(log_path) {
        if meta.len() > 500_000 {
            if let Ok(content) = std::fs::read_to_string(log_path) {
                let tail: String = content.lines().rev().take(2000).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
                let _ = std::fs::write(log_path, tail + "\n");
            }
        }
    }
}

/// Фоновый цикл: раз в `sync_poll_secs` дёшево проверяет, не изменилась
/// ли версия манифеста в общей папке, и только тогда выполняет полную
/// синхронизацию. Показ контента на экране никогда не ждёт эту проверку —
/// она работает полностью независимо в своём потоке.
pub async fn run(app: AppHandle, cfg: Arc<Mutex<KioskSettings>>, cache_root: PathBuf, log_path: PathBuf) {
    log_line(&log_path, "=== фоновая синхронизация запущена ===");
    loop {
        let (source_path, poll_secs) = {
            let c = cfg.lock().unwrap();
            (PathBuf::from(&c.content_source_path), c.sync_poll_secs.max(5))
        };

        match kiosk_sync::version_changed(&source_path, &cache_root) {
            Ok(false) => {
                // Ничего не изменилось — самый частый случай. Ни диск,
                // ни сеть контентом не трогаем. В лог не пишем, чтобы не
                // засорять его — только реальные события ниже.
            }
            Ok(true) => match kiosk_sync::sync_once(&source_path, &cache_root) {
                Ok(report) if report.did_swap => {
                    log_line(&log_path, &format!(
                        "контент обновлён: {} -> {} (докачано {}, удалено {}, без изменений {})",
                        report.previous_version.clone().unwrap_or_else(|| "—".into()),
                        report.new_version,
                        report.fetched.len(),
                        report.deleted.len(),
                        report.unchanged
                    ));
                    let _ = app.emit("kiosk://content-updated", &report);
                }
                Ok(_) => {}
                Err(e) => {
                    log_line(&log_path, &format!("ОШИБКА синхронизации: {e} (папка источника: {})", source_path.display()));
                }
            },
            Err(e) => {
                // Папка недоступна (сеть легла, магазин отвалился и т.п.).
                // Пишем в лог редко (раз в ~10 опросов), чтобы не заспамить
                // файл при долгой недоступности сети, но не терять сам факт.
                log_line(&log_path, &format!("папка источника недоступна: {e} (путь: {})", source_path.display()));
            }
        }

        trim_log_if_large(&log_path);
        tokio::time::sleep(Duration::from_secs(poll_secs)).await;
    }
}
