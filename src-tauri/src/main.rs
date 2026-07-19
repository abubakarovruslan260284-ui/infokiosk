// Инфокиоск (Tauri). Три обязанности этого файла:
//   1) поднять полноэкранное окно с киоском (как раньше делал Electron main.js);
//   2) в фоне, отдельным потоком, гонять kiosk_sync — опрос манифеста и
//      подмену локального кэша, НЕ трогая то, что уже показано на экране;
//   3) отдать фронтенду горстку команд: настройки, список слайдов из
//      кэша, принудительная синхронизация.
//
// Проверка цены по штрихкоду сюда НЕ входит: как и раньше, это обычный
// fetch() из веб-страницы прямо к HTTP-сервису 1С — этот путь короче,
// надёжнее и не требует посредника на стороне Rust.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings;
mod slides;
mod sync_loop;

use std::sync::Arc;
use tauri::Manager;

use settings::KioskSettings;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // Настройки читаем из settings.json рядом с exe — та же
            // договорённость, что была у Electron-версии, чтобы можно
            // было просто скопировать существующий файл на киоске.
            let exe_dir = settings::exe_dir(&handle);
            let settings_path = exe_dir.join("settings.json");
            let cfg = Arc::new(std::sync::Mutex::new(KioskSettings::load_or_default(&settings_path)));

            let cache_root = exe_dir.join("content-cache");
            std::fs::create_dir_all(&cache_root).ok();
            let log_path = exe_dir.join("sync.log");

            app.manage(AppState {
                settings_path: settings_path.clone(),
                cache_root: cache_root.clone(),
                log_path: log_path.clone(),
                cfg: cfg.clone(),
            });

            // Фоновый цикл синхронизации — отдельный tokio-раннтайм в
            // отдельном системном потоке. Он физически не может подвесить
            // окно приложения: WebView и синхронизация не делят один поток.
            let sync_handle = handle.clone();
            let sync_cfg = cfg.clone();
            let sync_cache_root = cache_root.clone();
            let sync_log_path = log_path.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .build()
                    .expect("не удалось создать tokio-раннтайм для фоновой синхронизации");
                rt.block_on(sync_loop::run(sync_handle, sync_cfg, sync_cache_root, sync_log_path));
            });

            // Полноэкранный киоск, без рамки/меню — как раньше.
            if let Some(win) = app.get_webview_window("main") {
                win.set_fullscreen(true).ok();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            save_settings_dialog,
            load_settings_dialog,
            list_active_slides,
            force_sync,
            read_diagnostics,
            fetch_price,
            exit_fullscreen,
            toggle_devtools,
        ])
        .run(tauri::generate_context!())
        .expect("ошибка при запуске Tauri-приложения");
}

pub struct AppState {
    pub settings_path: std::path::PathBuf,
    pub cache_root: std::path::PathBuf,
    pub log_path: std::path::PathBuf,
    pub cfg: Arc<std::sync::Mutex<KioskSettings>>,
}

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> KioskSettings {
    state.cfg.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: tauri::State<AppState>, data: KioskSettings) -> Result<(), String> {
    data.save(&state.settings_path).map_err(|e| e.to_string())?;
    *state.cfg.lock().unwrap() = data;
    Ok(())
}

/// Отдаёт фронтенду список слайдов из АКТИВНОГО локального кэша —
/// сеть/общая папка здесь вообще не участвуют, только диск.
#[tauri::command]
fn list_active_slides(state: tauri::State<AppState>) -> Vec<slides::SlideDto> {
    let active = kiosk_sync::active_cache_dir(&state.cache_root);
    slides::read_slides(&active)
}

/// Ручной запуск синхронизации (например, из панели настроек — кнопка
/// «Обновить контент сейчас»), не дожидаясь фонового таймера.
#[tauri::command]
fn force_sync(state: tauri::State<AppState>) -> Result<kiosk_sync::SyncReport, String> {
    let cfg = state.cfg.lock().unwrap().clone();
    let source = std::path::PathBuf::from(&cfg.content_source_path);
    kiosk_sync::sync_once(&source, &state.cache_root).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct Diagnostics {
    app_version: String,
    content_source_path: String,
    settings_path: String,
    cache_root: String,
    slide_count: usize,
    log_tail: String,
}

/// Всё, что нужно, чтобы разобраться "почему не работает", не гадая —
/// открывается по F4 (см. diagnostics.js). Читает settings.json,
/// содержимое активного кэша и последние строки sync.log напрямую с
/// диска, так что показывает РЕАЛЬНОЕ текущее состояние, а не то, что
/// когда-то было в памяти.
#[tauri::command]
fn read_diagnostics(app: tauri::AppHandle, state: tauri::State<AppState>) -> Diagnostics {
    let cfg = state.cfg.lock().unwrap().clone();
    let active = kiosk_sync::active_cache_dir(&state.cache_root);
    let slide_count = slides::read_slides(&active).len();
    let log_tail = std::fs::read_to_string(&state.log_path)
        .map(|s| {
            let lines: Vec<&str> = s.lines().rev().take(25).collect();
            lines.into_iter().rev().collect::<Vec<_>>().join("\n")
        })
        .unwrap_or_else(|_| "(лог пока пуст — синхронизация ещё не запускалась)".to_string());

    Diagnostics {
        app_version: app.package_info().version.to_string(),
        content_source_path: cfg.content_source_path,
        settings_path: state.settings_path.display().to_string(),
        cache_root: state.cache_root.display().to_string(),
        slide_count,
        log_tail,
    }
}

/// Аналог старой кнопки «Экспорт»: сохранить текущие настройки в файл,
/// который выбирает сам пользователь (флешка, другая папка и т.п.).
#[tauri::command]
async fn save_settings_dialog(app: tauri::AppHandle, data: KioskSettings) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;
    let file = app
        .dialog()
        .file()
        .add_filter("Файлы настроек", &["json"])
        .set_file_name("settings.json")
        .blocking_save_file();
    if let Some(path) = file {
        let p = path.as_path().ok_or("некорректный путь")?;
        data.save(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Аналог старой кнопки «Импорт»: выбрать существующий settings.json и
/// подхватить его значения в форму настроек.
#[tauri::command]
async fn load_settings_dialog(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    use tauri_plugin_dialog::DialogExt;
    let file = app
        .dialog()
        .file()
        .add_filter("Файлы настроек", &["json"])
        .blocking_pick_file();
    match file {
        None => Ok(serde_json::Value::Bool(false)),
        Some(path) => {
            let p = path.as_path().ok_or("некорректный путь")?;
            match std::fs::read_to_string(p) {
                Ok(s) => serde_json::from_str(&s).map_err(|_| "error".to_string()),
                Err(_) => Ok(serde_json::Value::String("error".to_string())),
            }
        }
    }
}

/// Запрос цены товара к HTTP-сервису 1С — выполняется на стороне Rust,
/// а не из браузера. Причина: WebView2 (движок этого приложения на
/// Windows) строго соблюдает CORS для запросов со страницы, а сервис 1С
/// не рассчитан отвечать на такие запросы браузера (нет заголовков
/// Access-Control-Allow-*). У серверных HTTP-запросов из Rust концепция
/// CORS вообще не применяется — это ограничение только браузеров.
#[tauri::command]
fn fetch_price(url: String, auth_token: String) -> Result<serde_json::Value, String> {
    let resp = ureq::get(&url)
        .set("Authorization", &format!("Basic {auth_token}"))
        .timeout(std::time::Duration::from_secs(6))
        .call()
        .map_err(|e| e.to_string())?;
    let body = resp.into_string().map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| format!("не удалось разобрать ответ сервера как JSON: {e}"))
}

#[tauri::command]
fn exit_fullscreen(window: tauri::Window) {
    window.set_fullscreen(false).ok();
}

#[tauri::command]
fn toggle_devtools(window: tauri::Window) {
    #[cfg(debug_assertions)]
    {
        if window.is_devtools_open() {
            window.close_devtools();
        } else {
            window.open_devtools();
        }
    }
    let _ = window;
}
