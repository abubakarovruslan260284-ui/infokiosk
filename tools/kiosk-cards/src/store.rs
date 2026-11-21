//! База карточек службы (data_dir/cards-db.json) и то, что из неё
//! публикуется для киосков (cards.json в общей папке).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::onec::Good;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Card {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub characteristic: String,
    pub price: f64,
    pub discounted_price: f64,
    #[serde(default)]
    pub discount_percent: f64,
    /// None — 1С пока не отдаёт остаток.
    #[serde(default)]
    pub stock: Option<f64>,
    #[serde(default)]
    pub group: Option<String>,
    /// Имя файла фото (одинаковое в data_dir/photos и в общей папке).
    pub photo: String,
    /// Товар нашёлся в 1С при последней проверке.
    #[serde(default = "yes")]
    pub found_in_1c: bool,
    pub created: String,
    pub updated: String,
    #[serde(default)]
    pub checked: String,
}

fn yes() -> bool {
    true
}

impl Card {
    /// Показывать ли карточку на киосках: товар есть в 1С и не закончился.
    pub fn visible(&self) -> bool {
        self.found_in_1c && self.stock.map_or(true, |s| s > 0.0)
    }

    /// Обновляет данные из 1С. true — что-то, видимое на киоске, изменилось.
    pub fn apply_good(&mut self, g: &Good, now: &str) -> bool {
        let before = (self.name.clone(), self.characteristic.clone(), self.price, self.discounted_price, self.visible(), self.group.clone());
        self.name = g.name.clone();
        self.characteristic = g.characteristic.clone();
        self.price = g.price;
        self.discounted_price = g.discounted_price;
        self.discount_percent = g.discount_percent;
        self.stock = g.stock;
        self.group = g.group.clone();
        self.found_in_1c = true;
        self.checked = now.to_string();
        let after = (self.name.clone(), self.characteristic.clone(), self.price, self.discounted_price, self.visible(), self.group.clone());
        let changed = before != after;
        if changed {
            self.updated = now.to_string();
        }
        changed
    }
}

/// Имя файла фото для кода: латиница/цифры как есть, остальное — в hex,
/// чтобы имя было безопасным для любой файловой системы и URL.
pub fn photo_name(code: &str) -> String {
    if !code.is_empty() && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        format!("card_{code}.jpg")
    } else {
        let hex: String = code.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        format!("card_x{hex}.jpg")
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    pub cards: BTreeMap<String, Card>,
}

impl Store {
    pub fn load(path: &Path) -> Store {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap())?;
        std::fs::rename(&tmp, path)
    }

    pub fn visible(&self) -> Vec<&Card> {
        self.cards.values().filter(|c| c.visible()).collect()
    }
}

/// Карточка в том виде, в каком её читает киоск (cards.json).
#[derive(Debug, Serialize, Deserialize)]
pub struct PublicCard {
    pub code: String,
    pub name: String,
    pub characteristic: String,
    pub price: f64,
    pub discounted_price: f64,
    pub discount_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub photo: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublicCards {
    pub generated: String,
    pub cards: Vec<PublicCard>,
}

impl PublicCards {
    pub fn from_store(store: &Store, generated: &str) -> PublicCards {
        PublicCards {
            generated: generated.to_string(),
            cards: store
                .visible()
                .into_iter()
                .map(|c| PublicCard {
                    code: c.code.clone(),
                    name: c.name.clone(),
                    characteristic: c.characteristic.clone(),
                    price: c.price,
                    discounted_price: c.discounted_price,
                    discount_percent: c.discount_percent,
                    group: c.group.clone(),
                    photo: c.photo.clone(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good(stock: Option<f64>) -> Good {
        Good { name: "Кисть".into(), characteristic: "".into(), price: 100.0, discounted_price: 95.0, discount_percent: 5.0, stock, group: None }
    }

    fn card() -> Card {
        Card {
            code: "1".into(), name: "Кисть".into(), characteristic: "".into(), price: 100.0, discounted_price: 95.0,
            discount_percent: 5.0, stock: None, group: None, photo: "card_1.jpg".into(), found_in_1c: true,
            created: "t0".into(), updated: "t0".into(), checked: "t0".into(),
        }
    }

    #[test]
    fn zero_stock_hides_card_and_unknown_stock_keeps_it() {
        let mut c = card();
        assert!(c.visible());
        assert!(c.apply_good(&good(Some(0.0)), "t1"));
        assert!(!c.visible());
        assert!(c.apply_good(&good(None), "t2"));
        assert!(c.visible());
        assert!(!c.apply_good(&good(None), "t3"), "ничего не поменялось");
        assert_eq!(c.updated, "t2");
        assert_eq!(c.checked, "t3");
    }

    #[test]
    fn price_change_is_a_change() {
        let mut c = card();
        let mut g = good(Some(3.0));
        g.discounted_price = 90.0;
        assert!(c.apply_good(&g, "t1"));
        assert_eq!(c.discounted_price, 90.0);
    }

    #[test]
    fn photo_names_are_safe() {
        assert_eq!(photo_name("4680269373655"), "card_4680269373655.jpg");
        assert_eq!(photo_name("СК-1"), "card_xd0a1d09a2d31.jpg");
    }
}
