use serde::{Deserialize, Serialize};
use std::path::Path;

/// Карточка товара так, как её публикует служба карточек (cards.json
/// в общей папке контента — киоск получает его вместе со слайдами).
#[derive(Debug, Clone, Deserialize)]
struct PublishedCard {
    code: String,
    name: String,
    #[serde(default)]
    characteristic: String,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    discounted_price: f64,
    #[serde(default)]
    group: Option<String>,
    photo: String,
    /// Ролик товара (card_<код>.mp4); тогда photo — его неподвижный кадр.
    #[serde(default)]
    video: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PublishedCards {
    #[serde(default)]
    cards: Vec<PublishedCard>,
}

/// То, что получает фронтенд: данные карточки + абсолютный путь к фото
/// в локальном кэше (как у слайдов — показывается через convertFileSrc).
#[derive(Debug, Clone, Serialize)]
pub struct CardDto {
    pub code: String,
    pub name: String,
    pub characteristic: String,
    pub price: f64,
    pub discounted_price: f64,
    pub group: Option<String>,
    /// Фото товара или неподвижный кадр ролика.
    pub path: String,
    /// Ролик товара, если есть.
    pub video_path: Option<String>,
}

/// Читает cards.json из АКТИВНОГО локального кэша. Нет файла / битый
/// файл — просто пустой список (киоск показывает обычные слайды).
pub fn read_cards(active_dir: &Path) -> Vec<CardDto> {
    let Ok(text) = std::fs::read_to_string(active_dir.join("cards.json")) else {
        return vec![];
    };
    let Ok(parsed) = serde_json::from_str::<PublishedCards>(text.trim_start_matches('\u{feff}')) else {
        return vec![];
    };
    parsed
        .cards
        .into_iter()
        .filter_map(|c| {
            // имена файлов — только простые имена внутри кэша
            let simple = |n: &str| !(n.contains('/') || n.contains('\\') || n.contains(".."));
            if !simple(&c.photo) {
                return None;
            }
            let path = active_dir.join(&c.photo);
            if !path.is_file() {
                return None;
            }
            let video_path = c
                .video
                .as_deref()
                .filter(|v| simple(v))
                .map(|v| active_dir.join(v))
                .filter(|p| p.is_file())
                .map(|p| p.to_string_lossy().to_string());
            Some(CardDto {
                code: c.code,
                name: c.name,
                characteristic: c.characteristic,
                price: c.price,
                discounted_price: c.discounted_price,
                group: c.group,
                path: path.to_string_lossy().to_string(),
                video_path,
            })
        })
        .collect()
}
