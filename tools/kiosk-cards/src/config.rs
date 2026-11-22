//! Настройки службы: файл kiosk-cards.json рядом с kiosk_cards.exe.
//! Если файла нет — служба создаёт шаблон и просит его заполнить.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Папка Google Диска (синхронизированная «Google Диском для
    /// компьютера»), куда айфон кладёт снимки.
    pub inbox_dir: String,
    /// Общая папка киосков — туда кладутся карточки, cards.json и манифест.
    pub output_dir: String,
    /// Локальная рабочая папка службы: база карточек, обработанные фото,
    /// архив исходных снимков, журнал.
    pub data_dir: String,
    /// HTTP-сервис 1С (тот же, что в настройках киоска).
    pub api_url: String,
    pub login: String,
    pub password: String,
    #[serde(default = "d_poll")]
    pub poll_seconds: u64,
    /// Как часто перепроверять цены и остатки всех карточек в 1С.
    #[serde(default = "d_refresh")]
    pub price_refresh_minutes: u64,
    /// Больше не используется: одиночный ролик или фото обложки служба
    /// через 2 минуты превращает в карточку сама.
    #[serde(default = "d_orphan")]
    pub orphan_minutes: u64,
    /// Размер фото в карточке по длинной стороне, пикселей.
    #[serde(default = "d_side")]
    pub photo_max_side: u32,
    /// Больше не используется (штрихкод по фото не сверяется); оставлено,
    /// чтобы старые файлы настроек читались без ошибок.
    #[serde(default = "d_true")]
    pub require_label_match: bool,
    /// Путь к ffmpeg.exe (перекодирует ролики с айфона в H.264, который
    /// понимают все киоски, включая Windows 7). Пусто — ffmpeg.exe рядом
    /// со службой.
    #[serde(default)]
    pub ffmpeg_path: String,
    /// Размер ролика по длинной стороне, пикселей (меньше — легче слабым киоскам).
    #[serde(default = "d_vside")]
    pub video_max_side: u32,
    /// Длина ролика в карточке, секунд (лишнее обрезается).
    #[serde(default = "d_vsec")]
    pub video_seconds: f64,
    /// true — ролик «туда-обратно» (бесшовное зацикливание, длина ×2).
    #[serde(default)]
    pub video_boomerang: bool,
    /// Сколько дней хранить исходники с телефона в archive (0 — вечно).
    #[serde(default = "d_archive")]
    pub archive_days: u64,
}

fn d_poll() -> u64 { 20 }
fn d_refresh() -> u64 { 60 }
fn d_orphan() -> u64 { 30 }
fn d_side() -> u32 { 1400 }
fn d_true() -> bool { true }
fn d_vside() -> u32 { 720 }
fn d_vsec() -> f64 { 3.0 }
fn d_archive() -> u64 { 60 }

impl Default for Config {
    fn default() -> Self {
        Config {
            inbox_dir: r"G:\Мой диск\Карточки Товаров".to_string(),
            output_dir: r"\\wdmycloud\Public\InfoContent".to_string(),
            data_dir: r"C:\KioskCards".to_string(),
            api_url: "http://192.168.0.14/UT_2017/hs/infokiosk".to_string(),
            login: String::new(),
            password: String::new(),
            poll_seconds: d_poll(),
            price_refresh_minutes: d_refresh(),
            orphan_minutes: d_orphan(),
            photo_max_side: d_side(),
            require_label_match: true,
            ffmpeg_path: String::new(),
            video_max_side: d_vside(),
            video_seconds: d_vsec(),
            video_boomerang: false,
            archive_days: d_archive(),
        }
    }
}

impl Config {
    pub fn path_next_to_exe() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kiosk-cards.json")
    }

    /// Ok(Some) — настройки прочитаны; Ok(None) — создан шаблон.
    pub fn load_or_create(path: &Path) -> Result<Option<Config>, String> {
        if !path.exists() {
            let text = serde_json::to_string_pretty(&Config::default()).unwrap();
            std::fs::write(path, text).map_err(|e| format!("не удалось создать {}: {e}", path.display()))?;
            return Ok(None);
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("не удалось прочитать {}: {e}", path.display()))?;
        let cfg: Config = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("ошибка в {}: {e}\n(частая причина — одиночная \\ в пути: в этом файле пишите \\\\)", path.display()))?;
        Ok(Some(cfg))
    }

    pub fn inbox(&self) -> PathBuf { PathBuf::from(&self.inbox_dir) }
    pub fn output(&self) -> PathBuf { PathBuf::from(&self.output_dir) }
    pub fn data(&self) -> PathBuf { PathBuf::from(&self.data_dir) }
    pub fn errors_dir(&self) -> PathBuf { self.inbox().join("_Ошибки") }

    pub fn ffmpeg(&self) -> PathBuf {
        if !self.ffmpeg_path.trim().is_empty() {
            return PathBuf::from(self.ffmpeg_path.trim());
        }
        let near = Config::path_next_to_exe().with_file_name(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
        if near.exists() {
            near
        } else {
            PathBuf::from("ffmpeg")
        }
    }
}
