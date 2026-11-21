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
    /// Сколько ждать второй снимок пары, прежде чем отправить одиночку в ошибки.
    #[serde(default = "d_orphan")]
    pub orphan_minutes: u64,
    /// Размер фото в карточке по длинной стороне, пикселей.
    #[serde(default = "d_side")]
    pub photo_max_side: u32,
    /// true — карточка создаётся только если штрихкод на этикетке
    /// распознан и совпал с именем файла.
    #[serde(default = "d_true")]
    pub require_label_match: bool,
}

fn d_poll() -> u64 { 20 }
fn d_refresh() -> u64 { 60 }
fn d_orphan() -> u64 { 30 }
fn d_side() -> u32 { 1400 }
fn d_true() -> bool { true }

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
}
