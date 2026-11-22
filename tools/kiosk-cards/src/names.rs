//! Разбор имён файлов, которые присылает страница телефона.
//!
//! Шаблон:  <штрихкод>__<дата-время>__<вид>.<расширение>
//!   вид = video (ролик товара, .mp4/.mov), photo (снимок товара, .jpg)
//!         или cover (фото обложки — картинка карточки, .jpg).
//! Например: 4680269373655__20261121-153012__video.mp4
//!           4680269373655__20261121-153012__cover.jpg
//!
//! Штрихкод берётся из имени (его вводит кольцо-сканер). Принимаются и
//! русские пометки (видео / товар / обложка), и цифры (1 — товар,
//! 2 — обложка). Старое имя label тоже считается обложкой. Регистр не важен.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Ролик или снимок товара.
    Photo,
    /// Фото обложки — картинка карточки и заставка ролика.
    Cover,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedName {
    pub code: String,
    pub stamp: String,
    pub kind: Kind,
    /// Файл — видеоролик (а не фото).
    pub video: bool,
}

impl ParsedName {
    /// Ключ пары: один товар, снятый в один заход.
    pub fn group_key(&self) -> String {
        format!("{}__{}", self.code, self.stamp)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameProblem {
    /// Не наш файл (не картинка) — просто не трогаем.
    NotImage,
    /// HEIC/HEIF — айфон не перевёл снимок в JPEG.
    Heic,
    /// Картинка, но имя не по шаблону.
    BadPattern,
}

pub fn parse(file_name: &str) -> Result<ParsedName, NameProblem> {
    let lower = file_name.to_lowercase();
    let (stem, ext) = match lower.rsplit_once('.') {
        Some((s, e)) => (s, e),
        None => return Err(NameProblem::NotImage),
    };
    let video = match ext {
        "jpg" | "jpeg" | "png" => false,
        "mp4" | "mov" | "m4v" => true,
        "heic" | "heif" => return Err(NameProblem::Heic),
        _ => return Err(NameProblem::NotImage),
    };
    // Берём исходное (не приведённое к нижнему регистру) имя для кода —
    // в кодах 1С бывают буквы.
    let orig_stem = &file_name[..stem.len()];
    let parts: Vec<&str> = orig_stem.split("__").map(|p| p.trim()).collect();
    if parts.len() != 3 {
        return Err(NameProblem::BadPattern);
    }
    let code = parts[0];
    let stamp = parts[1];
    let kind = match parts[2].to_lowercase().as_str() {
        "photo" | "foto" | "товар" | "tovar" | "p" | "1" | "video" | "видео" | "ролик" | "v" => Kind::Photo,
        "cover" | "обложка" | "oblozhka" | "c" | "label" | "этикетка" | "etiketka" | "l" | "2" => Kind::Cover,
        _ => return Err(NameProblem::BadPattern),
    };
    if code.is_empty() || !code.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return Err(NameProblem::BadPattern);
    }
    if stamp.is_empty() || !stamp.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '_' || c == 'T' || c == '.') {
        return Err(NameProblem::BadPattern);
    }
    if kind == Kind::Cover && video {
        // обложка — это фото
        return Err(NameProblem::BadPattern);
    }
    Ok(ParsedName { code: code.to_string(), stamp: stamp.to_string(), kind, video })
}

/// Приводит код к виду для сравнения: только буквы/цифры, верхний регистр.
pub fn normalize_code(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_uppercase()).collect()
}

/// Совпадают ли код из имени файла и код, распознанный на этикетке.
/// Учитывает, что UPC-A (12 цифр) и EAN-13 с ведущим нулём — один и тот
/// же штрихкод.
pub fn codes_match(a: &str, b: &str) -> bool {
    let a = normalize_code(a);
    let b = normalize_code(b);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let all_digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if all_digits(&a) && all_digits(&b) {
        let ta = a.trim_start_matches('0');
        let tb = b.trim_start_matches('0');
        return !ta.is_empty() && ta == tb;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_normal_names() {
        let p = parse("4680269373655__20261121-153012__photo.jpg").unwrap();
        assert_eq!(p.code, "4680269373655");
        assert_eq!(p.stamp, "20261121-153012");
        assert_eq!(p.kind, Kind::Photo);
        let l = parse("4680269373655__20261121-153012__COVER.JPG").unwrap();
        assert_eq!(l.kind, Kind::Cover);
        let l = parse("4680269373655__20261121-153012__LABEL.JPG").unwrap();
        assert_eq!(l.kind, Kind::Cover);
        assert_eq!(p.group_key(), l.group_key());
    }

    #[test]
    fn accepts_video_of_product() {
        let v = parse("4680269373655__20261121-153012__video.MOV").unwrap();
        assert_eq!(v.kind, Kind::Photo);
        assert!(v.video);
        assert!(parse("1__2__ролик.mp4").unwrap().video);
        assert_eq!(parse("1__2__label.mp4"), Err(NameProblem::BadPattern));
    }

    #[test]
    fn accepts_russian_and_digit_kinds() {
        assert_eq!(parse("123__20261121__товар.jpeg").unwrap().kind, Kind::Photo);
        assert_eq!(parse("123__20261121__Этикетка.jpg").unwrap().kind, Kind::Cover);
        assert_eq!(parse("123__20261121__Обложка.jpg").unwrap().kind, Kind::Cover);
        assert_eq!(parse("123__20261121__2.png").unwrap().kind, Kind::Cover);
    }

    #[test]
    fn rejects_bad_names() {
        assert_eq!(parse("IMG_0001.JPG"), Err(NameProblem::BadPattern));
        assert_eq!(parse("123__20261121.jpg"), Err(NameProblem::BadPattern));
        assert_eq!(parse("123__20261121__photo.heic"), Err(NameProblem::Heic));
        assert_eq!(parse("список.csv"), Err(NameProblem::NotImage));
        assert_eq!(parse("12 3__20261121__photo.jpg"), Err(NameProblem::BadPattern));
    }

    #[test]
    fn code_matching() {
        assert!(codes_match("4680269373655", "4680269373655"));
        assert!(codes_match("012345678905", "0012345678905"));
        assert!(codes_match("ск-00022825", "СК00022825"));
        assert!(!codes_match("4680269373655", "4680269373656"));
        assert!(!codes_match("", ""));
    }
}
