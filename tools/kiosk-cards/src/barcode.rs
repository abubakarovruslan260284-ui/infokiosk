//! Распознавание штрихкода на снимке этикетки.
//!
//! Снимок с айфона большой (12 Мп), штрихкод занимает его часть и может
//! быть повёрнут. Поэтому пробуем несколько масштабов; поворот на 90°
//! библиотека перебирает сама (режим TryHarder).

use image::DynamicImage;
use rxing::{BarcodeFormat, DecodeHints};
use std::collections::HashSet;

fn linear_formats() -> HashSet<BarcodeFormat> {
    HashSet::from([
        BarcodeFormat::EAN_13,
        BarcodeFormat::EAN_8,
        BarcodeFormat::UPC_A,
        BarcodeFormat::UPC_E,
        BarcodeFormat::CODE_128,
        BarcodeFormat::CODE_39,
        BarcodeFormat::CODE_93,
        BarcodeFormat::ITF,
        BarcodeFormat::CODABAR,
    ])
}

/// Все штрихкоды, которые удалось прочитать на картинке (без повторов).
pub fn decode_all(img: &DynamicImage) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let longest = img.width().max(img.height());
    // От крупного к мелкому: мелкий штрихкод лучше читается в большом
    // разрешении, размытый — в уменьшенном.
    // Сначала средний масштаб — на нём обычно всё читается и это быстро;
    // потом крупнее (мелкий штрихкод) и мельче (размытый снимок).
    let mut targets: Vec<u32> = vec![1600, 2400, longest.min(3200), 1000];
    targets.retain(|t| *t <= longest);
    if targets.is_empty() {
        targets.push(longest);
    }
    let mut uniq = Vec::new();
    for t in targets {
        if !uniq.contains(&t) {
            uniq.push(t);
        }
    }
    let targets = uniq;

    for target in targets {
        let scaled = if target < longest {
            img.resize(target, target, image::imageops::FilterType::Triangle)
        } else {
            img.clone()
        };
        let luma = scaled.to_luma8();
        let (w, h) = luma.dimensions();
        let raw = luma.into_raw();

        let mut hints = DecodeHints::default();
        hints.TryHarder = Some(true);
        hints.PossibleFormats = Some(linear_formats());
        if let Ok(results) = rxing::helpers::detect_multiple_in_luma_with_hints(raw.clone(), w, h, &mut hints) {
            for r in results {
                push_unique(&mut found, r.getText());
            }
        }
        if found.is_empty() {
            let mut hints = DecodeHints::default();
            hints.TryHarder = Some(true);
            hints.PossibleFormats = Some(linear_formats());
            if let Ok(r) = rxing::helpers::detect_in_luma_filtered_with_hints(raw, w, h, None, &mut hints) {
                push_unique(&mut found, r.getText());
            }
        }
        if !found.is_empty() {
            break;
        }
    }
    found
}

fn push_unique(v: &mut Vec<String>, s: &str) {
    let s = s.trim().to_string();
    if !s.is_empty() && !v.contains(&s) {
        v.push(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Luma};
    use rxing::Writer;

    /// Рисует настоящий EAN-13 (через кодировщик той же библиотеки) на
    /// «фото» побольше, с полями и шумом, как этикетка на снимке.
    fn synthetic_label(code: &str, scale: u32, rotate: bool) -> DynamicImage {
        let writer = rxing::MultiFormatWriter;
        let matrix = writer.encode(code, &BarcodeFormat::EAN_13, 0, 0).unwrap();
        let (mw, mh) = (matrix.getWidth(), matrix.getHeight().max(1));
        let bar_h = 60u32;
        let (cw, ch) = (mw * scale + 400, bar_h * scale + 600);
        let mut canvas = GrayImage::from_pixel(cw, ch, Luma([235u8]));
        for x in 0..mw * scale {
            let on = matrix.get(x / scale, 0);
            if on {
                for y in 0..bar_h * scale {
                    canvas.put_pixel(200 + x, 300 + y, Luma([20u8]));
                }
            }
        }
        // лёгкий «шум» бумаги
        for (i, p) in canvas.pixels_mut().enumerate() {
            let n = ((i * 2654435761usize) >> 13) % 17;
            p.0[0] = p.0[0].saturating_sub(n as u8);
        }
        let img = DynamicImage::ImageLuma8(canvas);
        if rotate {
            img.rotate90()
        } else {
            img
        }
    }

    #[test]
    fn reads_ean13_from_synthetic_label() {
        let img = synthetic_label("4680269373655", 4, false);
        assert_eq!(decode_all(&img), vec!["4680269373655".to_string()]);
    }

    #[test]
    fn reads_rotated_and_large_label() {
        let img = synthetic_label("4607001771562", 12, true);
        let codes = decode_all(&img);
        assert!(codes.contains(&"4607001771562".to_string()), "got {codes:?}");
    }

    #[test]
    fn empty_picture_gives_nothing() {
        let img = DynamicImage::ImageLuma8(GrayImage::from_pixel(800, 600, Luma([200u8])));
        assert!(decode_all(&img).is_empty());
    }
}
