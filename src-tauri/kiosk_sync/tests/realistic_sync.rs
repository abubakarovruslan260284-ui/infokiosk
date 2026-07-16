//! Интеграционный тест-бенчмарк: имитация реального набора контента
//! (20 фото + 3 коротких видео) и замер, сколько времени занимает
//! синхронизация с нуля и повторная синхронизация после правки одного файла.
//!
//! Это не микро-юнит-тест, а приближение к тому, что произойдёт на
//! реальном киоске: сколько секунд займёт "забрать всё в первый раз"
//! и сколько миллисекунд — "проверить, что нового нет".

use kiosk_sync::{sync_once, version_changed, Manifest};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn tempdir(tag: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let unique = format!(
        "kiosk_sync_bench_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    );
    let p = base.join(unique);
    fs::create_dir_all(&p).unwrap();
    p
}

/// Генерирует псевдослучайные, но детерминированные байты — заменитель
/// настоящих фото/видео, чтобы не тянуть тестовые ассеты из сети.
fn fake_content(seed: u64, size: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut out = Vec::with_capacity(size);
    for _ in 0..size {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        out.push((state >> 33) as u8);
    }
    out
}

fn publish_realistic_library(source: &Path, version: &str) {
    fs::create_dir_all(source).unwrap();
    // 20 фото по ~800 КБ (типичный сжатый JPG под экран 1080x1920)
    for i in 0..20 {
        let data = fake_content(i, 800 * 1024);
        fs::write(source.join(format!("photo_{i:02}.jpg")), data).unwrap();
    }
    // 3 коротких видео по ~6 МБ (сжатое 10-секундное вертикальное видео)
    for i in 0..3 {
        let data = fake_content(1000 + i, 6 * 1024 * 1024);
        fs::write(source.join(format!("story_{i:02}.mp4")), data).unwrap();
    }
    let manifest = Manifest::from_directory(source, version).unwrap();
    manifest.save(&source.join("manifest.json")).unwrap();
}

#[test]
fn realistic_library_first_sync_and_incremental_update() {
    let source = tempdir("source");
    let cache = tempdir("cache");

    publish_realistic_library(&source, "2026-07-16-001");

    // --- первая синхронизация: тянем всё (≈34 МБ) ---
    let t0 = Instant::now();
    let report = sync_once(&source, &cache).unwrap();
    let first_sync_ms = t0.elapsed().as_millis();

    assert!(report.did_swap);
    assert_eq!(report.fetched.len(), 23, "должны были забрать все 23 файла");
    println!(
        "[bench] первая синхронизация (23 файла, ~34 МБ): {} мс ({} файлов)",
        first_sync_ms,
        report.fetched.len()
    );

    // --- опрос без изменений: должен быть практически мгновенным ---
    let t1 = Instant::now();
    let changed = version_changed(&source, &cache).unwrap();
    let poll_ms = t1.elapsed().as_millis();
    assert!(!changed);
    println!("[bench] опрос манифеста без изменений: {poll_ms} мс (changed={changed})");
    assert!(poll_ms < 50, "опрос версии должен быть почти мгновенным (<50мс), получили {poll_ms}мс");

    // --- публикуем правку: заменили 1 фото и добавили 1 новое видео ---
    let updated_photo = fake_content(999, 800 * 1024);
    fs::write(source.join("photo_05.jpg"), updated_photo).unwrap();
    let new_video = fake_content(2000, 6 * 1024 * 1024);
    fs::write(source.join("story_03.mp4"), new_video).unwrap();
    let manifest2 = Manifest::from_directory(&source, "2026-07-16-002").unwrap();
    manifest2.save(&source.join("manifest.json")).unwrap();

    let t2 = Instant::now();
    let changed2 = version_changed(&source, &cache).unwrap();
    let poll2_ms = t2.elapsed().as_millis();
    assert!(changed2);
    println!("[bench] опрос манифеста ПОСЛЕ изменения: {poll2_ms} мс (changed={changed2})");

    let t3 = Instant::now();
    let report2 = sync_once(&source, &cache).unwrap();
    let incremental_ms = t3.elapsed().as_millis();

    assert!(report2.did_swap);
    assert_eq!(
        sorted(report2.fetched.clone()),
        vec!["photo_05.jpg".to_string(), "story_03.mp4".to_string()],
        "должны были докачаться ТОЛЬКО 2 изменённых/новых файла из 24"
    );
    assert_eq!(report2.unchanged, 22, "22 файла из 24 должны были остаться нетронутыми");
    println!(
        "[bench] инкрементальная синхронизация (2 файла из 24 изменились): {incremental_ms} мс"
    );

    assert!(
        incremental_ms < first_sync_ms,
        "инкрементальная синхронизация ({incremental_ms}мс) должна быть заметно быстрее первой ({first_sync_ms}мс)"
    );

    fs::remove_dir_all(&source).ok();
    fs::remove_dir_all(&cache).ok();
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}
