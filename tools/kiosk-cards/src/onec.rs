//! Запросы к HTTP-сервису 1С: GET {api_url}/getgood/{штрихкод}.
//!
//! Тот же запрос, что делает киоск при сканировании. Поля ответа,
//! которые нам нужны: name, characteristic, price, discounted_price.
//! Необязательные (появятся, когда их добавят в 1С): stock — остаток,
//! group — родительская группа номенклатуры.

use base64::Engine;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Good {
    pub name: String,
    pub characteristic: String,
    pub price: f64,
    pub discounted_price: f64,
    pub discount_percent: f64,
    /// None — 1С пока не отдаёт остаток (тогда карточку не прячем).
    pub stock: Option<f64>,
    pub group: Option<String>,
}

#[derive(Debug)]
pub enum OneCError {
    /// 1С недоступна / сеть / таймаут — попробуем позже, файлы не трогаем.
    Unavailable(String),
    /// 1С ответила, но такого товара нет.
    NotFound,
    /// Ответ не похож на товар.
    BadAnswer(String),
}

impl std::fmt::Display for OneCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OneCError::Unavailable(e) => write!(f, "1С недоступна: {e}"),
            OneCError::NotFound => write!(f, "товар не найден в 1С"),
            OneCError::BadAnswer(e) => write!(f, "непонятный ответ 1С: {e}"),
        }
    }
}

pub struct OneC {
    api_url: String,
    auth: String,
}

impl OneC {
    pub fn new(api_url: &str, login: &str, password: &str) -> Self {
        let token = base64::engine::general_purpose::STANDARD.encode(format!("{login}:{password}").as_bytes());
        OneC { api_url: api_url.trim_end_matches('/').to_string(), auth: format!("Basic {token}") }
    }

    pub fn get_good(&self, code: &str) -> Result<Good, OneCError> {
        let url = format!("{}/getgood/{}", self.api_url, code);
        let resp = ureq::get(&url)
            .set("Authorization", &self.auth)
            .timeout(Duration::from_secs(15))
            .call();
        let body = match resp {
            Ok(r) => r.into_string().map_err(|e| OneCError::Unavailable(e.to_string()))?,
            Err(ureq::Error::Status(404, _)) => return Err(OneCError::NotFound),
            Err(ureq::Error::Status(code @ (401 | 403), _)) => {
                return Err(OneCError::Unavailable(format!("HTTP {code}: 1С не пустила — проверьте логин и пароль в kiosk-cards.json")));
            }
            Err(ureq::Error::Status(code, r)) => {
                let text = r.into_string().unwrap_or_default();
                // Многие HTTP-сервисы 1С на «не найдено» отвечают 400/500
                // с текстом — считаем это «не найден», а не аварией сети.
                if code == 400 || text.to_lowercase().contains("не найден") {
                    return Err(OneCError::NotFound);
                }
                return Err(OneCError::Unavailable(format!("HTTP {code}: {}", text.chars().take(200).collect::<String>())));
            }
            Err(e) => return Err(OneCError::Unavailable(e.to_string())),
        };
        parse_good(&body)
    }
}

pub fn parse_good(body: &str) -> Result<Good, OneCError> {
    let v: Value = serde_json::from_str(body.trim_start_matches('\u{feff}'))
        .map_err(|e| OneCError::BadAnswer(format!("{e}: {}", body.chars().take(120).collect::<String>())))?;
    let v = match &v {
        Value::Array(a) if !a.is_empty() => a[0].clone(),
        Value::Array(_) | Value::Null => return Err(OneCError::NotFound),
        _ => v,
    };
    let name = str_field(&v, &["name", "наименование"]).unwrap_or_default();
    if name.trim().is_empty() {
        return Err(OneCError::NotFound);
    }
    let price = num_field(&v, &["price", "цена"]).unwrap_or(0.0);
    let discounted = num_field(&v, &["discounted_price", "цена_по_карте"]).unwrap_or(price);
    Ok(Good {
        name: name.trim().to_string(),
        characteristic: str_field(&v, &["characteristic", "характеристика"]).unwrap_or_default().trim().to_string(),
        price,
        discounted_price: discounted,
        discount_percent: num_field(&v, &["discount_percent"]).unwrap_or(0.0),
        stock: num_field(&v, &["stock", "ostatok", "остаток", "quantity"]),
        group: str_field(&v, &["group", "parent", "группа", "родитель"]).filter(|s| !s.trim().is_empty()),
    })
}

fn get<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| v.get(*k)).filter(|x| !x.is_null())
}

fn str_field(v: &Value, keys: &[&str]) -> Option<String> {
    match get(v, keys)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn num_field(v: &Value, keys: &[&str]) -> Option<f64> {
    match get(v, keys)? {
        Value::Number(n) => n.as_f64(),
        // 1С часто отдаёт числа строкой, иногда с запятой и пробелами.
        Value::String(s) => s.replace(['\u{a0}', ' '], "").replace(',', ".").parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_kiosk_answer() {
        let g = parse_good(r#"{"code":"СК-00022825","name":"\"BUBBLE TIME\" Краситель","artikul":"","barcode":"4680269373655","discount_percent":"5","price":"254","discounted_price":"241.10","characteristic":""}"#).unwrap();
        assert_eq!(g.price, 254.0);
        assert_eq!(g.discounted_price, 241.10);
        assert_eq!(g.stock, None);
        assert_eq!(g.group, None);
    }

    #[test]
    fn parses_future_fields() {
        let g = parse_good(r#"{"name":"Кисть","price":100,"discounted_price":"95,50","stock":"0","group":"Кисти"}"#).unwrap();
        assert_eq!(g.discounted_price, 95.5);
        assert_eq!(g.stock, Some(0.0));
        assert_eq!(g.group.as_deref(), Some("Кисти"));
    }

    #[test]
    fn empty_answers_are_not_found() {
        assert!(matches!(parse_good("{}"), Err(OneCError::NotFound)));
        assert!(matches!(parse_good("[]"), Err(OneCError::NotFound)));
        assert!(matches!(parse_good("\u{feff}{\"name\":\"\"}"), Err(OneCError::NotFound)));
    }
}
