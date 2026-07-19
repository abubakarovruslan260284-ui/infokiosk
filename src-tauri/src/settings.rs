use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Настройки киоска. Поля `url_products`/`login`/`password` — те же,
/// что были в старом Electron-варианте (для проверки цен через 1С).
/// Новые поля — только для локальной синхронизации контента:
/// `content_source_path` — это путь к общей сетевой папке (или папке
/// «Издателя»), `sync_poll_secs` — как часто дёшево проверять манифест.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KioskSettings {
    pub url_products: String,
    #[serde(default = "default_login")]
    pub login: String,
    #[serde(default = "default_password")]
    pub password: String,
    #[serde(default = "default_content_source")]
    pub content_source_path: String,
    #[serde(default = "default_poll_secs")]
    pub sync_poll_secs: u64,
    #[serde(default = "default_slide_seconds")]
    pub slide_seconds: u64,
    #[serde(default)]
    pub show_logo: bool,
    #[serde(default = "default_border_mode")]
    pub border_mode: String, // "off" | "solid" | "rainbow"
    #[serde(default = "default_border_color")]
    pub border_color: String, // hex, напр. "#e73a7c" — для режима "solid"
    #[serde(default = "default_border_intensity")]
    pub border_intensity: f64, // 0.0..1.0 — яркость/сила свечения
    #[serde(default = "default_border_speed")]
    pub border_speed_sec: f64, // длительность одного цикла анимации, сек
}

fn default_content_source() -> String {
    // Общая сетевая папка с контентом (магазин ИрсКом). Для другого
    // магазина/сети это поле просто меняется в settings.json на месте —
    // приложение подхватит новый путь на следующем опросе.
    r"\\wdmycloud\Public\InfoContent".to_string()
}
fn default_login() -> String {
    "АпГрейд".to_string()
}
fn default_password() -> String {
    "7STREkoza7".to_string()
}
fn default_poll_secs() -> u64 {
    20
}
fn default_slide_seconds() -> u64 {
    6
}
fn default_border_mode() -> String {
    "rainbow".to_string()
}
fn default_border_color() -> String {
    "#e73a7c".to_string()
}
fn default_border_intensity() -> f64 {
    0.7
}
fn default_border_speed() -> f64 {
    6.0
}

impl Default for KioskSettings {
    fn default() -> Self {
        KioskSettings {
            url_products: "http://192.168.0.14/UT_2017/hs/infokiosk".to_string(),
            login: default_login(),
            password: default_password(),
            content_source_path: default_content_source(),
            sync_poll_secs: default_poll_secs(),
            slide_seconds: default_slide_seconds(),
            show_logo: false,
            border_mode: default_border_mode(),
            border_color: default_border_color(),
            border_intensity: default_border_intensity(),
            border_speed_sec: default_border_speed(),
        }
    }
}

impl KioskSettings {
    pub fn load_or_default(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let data = serde_json::to_string_pretty(self).unwrap();
        std::fs::write(path, data)
    }
}

/// Папка данных приложения (per-user, всегда доступна на запись —
/// в отличие от каталога рядом с установленным .exe, который на Windows
/// обычно лежит в Program Files и требует прав администратора). Сюда
/// кладутся `settings.json` и `content-cache/`.
pub fn exe_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_local_data_dir()
        .unwrap_or_else(|_| std::env::current_exe().unwrap().parent().unwrap().to_path_buf())
}

use tauri::Manager;
