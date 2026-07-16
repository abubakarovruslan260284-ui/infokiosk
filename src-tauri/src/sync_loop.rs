use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::settings::KioskSettings;

/// Фоновый цикл: раз в `sync_poll_secs` дёшево проверяет, не изменилась
/// ли версия манифеста в общей папке, и только тогда выполняет полную
/// синхронизацию. Показ контента на экране никогда не ждёт эту проверку —
/// она работает полностью независимо в своём потоке.
pub async fn run(app: AppHandle, cfg: Arc<Mutex<KioskSettings>>, cache_root: PathBuf) {
    loop {
        let (source_path, poll_secs) = {
            let c = cfg.lock().unwrap();
            (PathBuf::from(&c.content_source_path), c.sync_poll_secs.max(5))
        };

        match kiosk_sync::version_changed(&source_path, &cache_root) {
            Ok(false) => {
                // Ничего не изменилось — самый частый случай. Ни диск,
                // ни сеть контентом не трогаем.
            }
            Ok(true) => match kiosk_sync::sync_once(&source_path, &cache_root) {
                Ok(report) if report.did_swap => {
                    eprintln!(
                        "контент обновлён: {} -> {} (докачано {}, удалено {}, без изменений {})",
                        report.previous_version.clone().unwrap_or_else(|| "—".into()),
                        report.new_version,
                        report.fetched.len(),
                        report.deleted.len(),
                        report.unchanged
                    );
                    // Сообщаем фронтенду, что в кэше есть кое-что новое.
                    // Фронтенд сам решает, КОГДА безопасно перечитать
                    // список слайдов (см. content.js: не в момент показа
                    // карточки цены), поэтому здесь мы просто уведомляем.
                    let _ = app.emit("kiosk://content-updated", &report);
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("[sync] синхронизация не удалась: {e}");
                }
            },
            Err(e) => {
                // Папка недоступна (сеть легла, магазин отвалился и т.п.) —
                // это НЕ ошибка приложения. Молча ждём следующего опроса,
                // работая на том, что уже есть в локальном кэше.
                eprintln!("[sync] источник контента недоступен: {e}");
            }
        }

        tokio::time::sleep(Duration::from_secs(poll_secs)).await;
    }
}
