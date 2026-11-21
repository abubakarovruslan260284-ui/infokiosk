//! Публикация в общую папку киосков (\\wdmycloud\Public\InfoContent).
//!
//! Порядок важен: сначала фото, потом cards.json и в самом конце
//! manifest.json. Киоски смотрят только на версию манифеста, поэтому
//! новую версию они увидят, когда все файлы уже на месте.
//!
//! Манифест строится по ВСЕЙ папке — вместе с рекламными картинками и
//! видео, которые сотрудники кладут туда вручную. Хэши файлов кэшируются
//! (по имени, размеру и времени изменения), чтобы не перечитывать каждый
//! раз всю папку по сети.

use kiosk_sync::{FileEntry, Manifest};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::store::{PublicCards, Store};

pub const CARDS_FILE: &str = "cards.json";
const MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HashCache {
    entries: BTreeMap<String, CachedHash>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedHash {
    size: u64,
    mtime_ms: u128,
    sha256: String,
}

impl HashCache {
    pub fn load(path: &Path) -> HashCache {
        fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub fn save(&self, path: &Path) {
        let _ = fs::write(path, serde_json::to_string(self).unwrap());
    }
    fn hash(&mut self, path: &Path, name: &str, size: u64, mtime_ms: u128) -> Result<String, String> {
        if let Some(c) = self.entries.get(name) {
            if c.size == size && c.mtime_ms == mtime_ms {
                return Ok(c.sha256.clone());
            }
        }
        let sha = kiosk_sync::manifest::hash_file(path).map_err(|e| e.to_string())?;
        self.entries.insert(name.to_string(), CachedHash { size, mtime_ms, sha256: sha.clone() });
        Ok(sha)
    }
}

/// Файлы, которые попадают в манифест: всё, что умеет показывать киоск,
/// плюс cards.json.
pub fn is_published_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower == MANIFEST_FILE || lower.starts_with('.') || lower.starts_with('~') {
        return false;
    }
    lower == CARDS_FILE
        || [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".mp4", ".webm"].iter().any(|e| lower.ends_with(e))
}

fn mtime_ms(meta: &fs::Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// «Слепок» общей папки: имена, размеры и время изменения публикуемых
/// файлов. Если он изменился с прошлой публикации (например, кто-то
/// вручную положил рекламу) — манифест надо пересобрать.
pub fn output_signature(out: &Path) -> Result<String, String> {
    let mut items = Vec::new();
    for e in fs::read_dir(out).map_err(|e| format!("{}: {e}", out.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        if !is_published_file(&name) {
            continue;
        }
        let meta = e.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() {
            continue;
        }
        items.push(format!("{name}|{}|{}", meta.len(), mtime_ms(&meta)));
    }
    items.sort();
    Ok(items.join("\n"))
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    fs::write(&tmp, data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

fn copy_atomic(from: &Path, to: &Path) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.part", to.display()));
    fs::copy(from, &tmp).map_err(|e| format!("{} -> {}: {e}", from.display(), tmp.display()))?;
    fs::rename(&tmp, to).map_err(|e| format!("{}: {e}", to.display()))
}

pub struct PublishReport {
    pub copied: usize,
    pub removed: usize,
    pub cards: usize,
    pub files: usize,
    pub version: String,
}

/// Приводит общую папку в соответствие с базой карточек и пересобирает
/// манифест. photos_dir — где лежат готовые фото карточек.
pub fn publish(store: &Store, photos_dir: &Path, out: &Path, cache: &mut HashCache, now_human: &str, version: &str) -> Result<PublishReport, String> {
    if !out.is_dir() {
        return Err(format!("общая папка недоступна: {}", out.display()));
    }
    let public = PublicCards::from_store(store, now_human);
    let wanted: HashSet<&str> = public.cards.iter().map(|c| c.photo.as_str()).collect();

    // 1) Фото видимых карточек: докладываем новые/изменившиеся.
    let mut copied = 0;
    for c in &public.cards {
        let src = photos_dir.join(&c.photo);
        let dst = out.join(&c.photo);
        if !src.exists() {
            continue;
        }
        let same = match (fs::metadata(&src), fs::metadata(&dst)) {
            (Ok(a), Ok(b)) => a.len() == b.len() && kiosk_sync::manifest::hash_file(&src).ok() == cache_hash_of(cache, &dst, &c.photo),
            _ => false,
        };
        if !same {
            copy_atomic(&src, &dst)?;
            copied += 1;
        }
    }

    // 2) Убираем фото карточек, которые больше не показываются
    //    (закончился товар / удалён из 1С). Чужие файлы не трогаем —
    //    только наши card_*.jpg.
    let mut removed = 0;
    for e in fs::read_dir(out).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        let ours = name.starts_with("card_") && name.to_lowercase().ends_with(".jpg");
        let leftover = name.ends_with(".part") || name.ends_with(".tmp");
        if (ours && !wanted.contains(name.as_str())) || leftover {
            if fs::remove_file(e.path()).is_ok() && ours {
                removed += 1;
            }
        }
    }

    // 3) cards.json
    let json = serde_json::to_vec_pretty(&public).unwrap();
    write_atomic(&out.join(CARDS_FILE), &json)?;

    // 4) manifest.json по всей папке
    let mut files = Vec::new();
    for e in fs::read_dir(out).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        if !is_published_file(&name) {
            continue;
        }
        let meta = e.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() == 0 {
            continue;
        }
        let sha256 = cache.hash(&e.path(), &name, meta.len(), mtime_ms(&meta))?;
        files.push(FileEntry { name, sha256, size: meta.len(), order: None });
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let manifest = Manifest { version: version.to_string(), files };
    let mjson = serde_json::to_vec_pretty(&manifest).unwrap();
    write_atomic(&out.join(MANIFEST_FILE), &mjson)?;

    Ok(PublishReport { copied, removed, cards: public.cards.len(), files: manifest.files.len(), version: version.to_string() })
}

fn cache_hash_of(cache: &mut HashCache, path: &Path, name: &str) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    cache.hash(path, name, meta.len(), mtime_ms(&meta)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Card;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kiosk-cards-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn card(code: &str, stock: Option<f64>) -> Card {
        Card {
            code: code.into(), name: format!("Товар {code}"), characteristic: "".into(), price: 100.0, discounted_price: 95.0,
            discount_percent: 5.0, stock, group: None, photo: crate::store::photo_name(code), found_in_1c: true,
            created: "t".into(), updated: "t".into(), checked: "t".into(),
        }
    }

    #[test]
    fn publishes_visible_cards_keeps_ads_and_kiosk_can_sync() {
        let photos = tmpdir("photos");
        let out = tmpdir("out");
        let kiosk = tmpdir("kiosk");
        fs::write(photos.join("card_111.jpg"), b"photo-111").unwrap();
        fs::write(photos.join("card_222.jpg"), b"photo-222").unwrap();
        fs::write(out.join("01_реклама.jpg"), b"ad").unwrap();
        fs::write(out.join("card_999.jpg"), b"stale card").unwrap();

        let mut store = Store::default();
        store.cards.insert("111".into(), card("111", None));
        store.cards.insert("222".into(), card("222", Some(0.0))); // закончился

        let mut cache = HashCache::default();
        let r = publish(&store, &photos, &out, &mut cache, "сейчас", "v1").unwrap();
        assert_eq!(r.cards, 1);
        assert_eq!(r.copied, 1);
        assert_eq!(r.removed, 1);
        assert!(out.join("card_111.jpg").exists());
        assert!(!out.join("card_222.jpg").exists());
        assert!(!out.join("card_999.jpg").exists());
        assert!(out.join("01_реклама.jpg").exists(), "рекламу сотрудников не трогаем");

        // Киоск (тот же движок, что в приложении) забирает всё, включая cards.json.
        let rep = kiosk_sync::sync_once(&out, &kiosk).unwrap();
        assert!(rep.did_swap);
        let active = kiosk_sync::active_cache_dir(&kiosk);
        assert!(active.join("cards.json").exists());
        assert!(active.join("card_111.jpg").exists());
        assert!(active.join("01_реклама.jpg").exists());

        // Повторная публикация без изменений ничего не копирует.
        let r2 = publish(&store, &photos, &out, &mut cache, "сейчас", "v2").unwrap();
        assert_eq!(r2.copied, 0);
        assert_eq!(r2.removed, 0);
    }
}
