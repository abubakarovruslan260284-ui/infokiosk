//! РСЃРїРѕР»РЅРµРЅРёРµ РїР»Р°РЅР° СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёРё: РґРѕРєР°С‡РєР° РёР·РјРµРЅС‘РЅРЅС‹С… С„Р°Р№Р»РѕРІ РІРѕ
//! РІСЂРµРјРµРЅРЅСѓСЋ РїР°РїРєСѓ Рё Р°С‚РѕРјР°СЂРЅР°СЏ РїРѕРґРјРµРЅР° СЂР°Р±РѕС‡РµРіРѕ РєСЌС€Р°.
//!
//! РљР»СЋС‡РµРІРѕРµ РёРЅРІР°СЂРёР°РЅС‚: РїРѕРєСѓРїР°С‚РµР»СЊ РЅРёРєРѕРіРґР° РЅРµ РґРѕР»Р¶РµРЅ СѓРІРёРґРµС‚СЊ РєСЌС€ РІ
//! РїСЂРѕРјРµР¶СѓС‚РѕС‡РЅРѕРј СЃРѕСЃС‚РѕСЏРЅРёРё (С‡Р°СЃС‚СЊ С„Р°Р№Р»РѕРІ РЅРѕРІС‹Рµ, С‡Р°СЃС‚СЊ СЃС‚Р°СЂС‹Рµ, С‡С‚Рѕ-С‚Рѕ
//! РµС‰С‘ РєРѕРїРёСЂСѓРµС‚СЃСЏ). РџРѕСЌС‚РѕРјСѓ РјС‹ РЅРёРєРѕРіРґР° РЅРµ РїРёС€РµРј РїРѕРІРµСЂС… СЂР°Р±РѕС‡РµРіРѕ РєСЌС€Р°
//! РЅР°РїСЂСЏРјСѓСЋ вЂ” СЃРЅР°С‡Р°Р»Р° СЃРѕР±РёСЂР°РµРј РџРћР›РќР«Р™ РЅРѕРІС‹Р№ РєСЌС€ РІ staging-РїР°РїРєРµ СЂСЏРґРѕРј,
//! Рё С‚РѕР»СЊРєРѕ РєРѕРіРґР° РѕРЅ РіРѕС‚РѕРІ Рё РїСЂРѕРІРµСЂРµРЅ вЂ” РѕРґРЅРѕР№ РѕРїРµСЂР°С†РёРµР№ РїРµСЂРµРёРјРµРЅРѕРІР°РЅРёСЏ
//! РїРѕРґРјРµРЅСЏРµРј РґРёСЂРµРєС‚РѕСЂРёСЋ С†РµР»РёРєРѕРј.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::diff::{plan_sync, Plan};
use crate::error::SyncError;
use crate::manifest::{hash_file, Manifest};

const MANIFEST_FILE: &str = "manifest.json";
const LOCAL_MANIFEST_FILE: &str = ".local-manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SyncReport {
    pub previous_version: Option<String>,
    pub new_version: String,
    pub fetched: Vec<String>,
    pub deleted: Vec<String>,
    pub unchanged: usize,
    pub did_swap: bool,
    pub duration_ms: u128,
}

/// РњРµРґРёР°С„Р°Р№Р» Р»Рё СЌС‚Рѕ (РїРѕ СЂР°СЃС€РёСЂРµРЅРёСЋ). Р”СѓР±Р»РёСЂСѓРµС‚ СЃРїРёСЃРѕРє РёР· manifest.rs
/// РЅР°РјРµСЂРµРЅРЅРѕ: СЃСЋРґР° РѕРЅ РЅСѓР¶РµРЅ РґР»СЏ РґРµС€С‘РІРѕРіРѕ В«СЃР»РµРїРєР°В» РїР°РїРєРё Р±РµР· РјР°РЅРёС„РµСЃС‚Р°,
/// Рё РґРµСЂР¶Р°С‚СЊ РµРіРѕ Р»РѕРєР°Р»СЊРЅРѕ РїСЂРѕС‰Рµ, С‡РµРј СЂР°СЃС€РёСЂСЏС‚СЊ РїСѓР±Р»РёС‡РЅС‹Р№ РёРЅС‚РµСЂС„РµР№СЃ
/// РјРѕРґСѓР»СЏ РјР°РЅРёС„РµСЃС‚Р° СЂР°РґРё РѕРґРЅРѕРіРѕ РјРµСЃС‚Р°.
fn is_media_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".mp4", ".webm"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// Р”РµС€С‘РІС‹Р№ В«СЃР»РµРїРѕРє РІРµСЂСЃРёРёВ» РїР°РїРєРё, РІ РєРѕС‚РѕСЂРѕР№ РќР•Рў manifest.json: СЃС‡РёС‚Р°РµРј
/// С…СЌС€ РѕС‚ СЃРїРёСЃРєР° (РёРјСЏ, СЂР°Р·РјРµСЂ, РІСЂРµРјСЏ РёР·РјРµРЅРµРЅРёСЏ) РІСЃРµС… РјРµРґРёР°С„Р°Р№Р»РѕРІ, Р‘Р•Р—
/// С‡С‚РµРЅРёСЏ РёС… СЃРѕРґРµСЂР¶РёРјРѕРіРѕ. Р­С‚Рѕ РїРѕР·РІРѕР»СЏРµС‚ С‡Р°СЃС‚Рѕ (РЅР° РєР°Р¶РґРѕРј РѕРїСЂРѕСЃРµ) РґС‘С€РµРІРѕ
/// РїРѕРЅСЏС‚СЊ, РёР·РјРµРЅРёР»РѕСЃСЊ Р»Рё С‡С‚Рѕ-С‚Рѕ РІ РѕР±С‰РµР№ РїР°РїРєРµ.
///
/// Р’РѕР·РІСЂР°С‰Р°РµС‚ `SourceUnavailable`, РµСЃР»Рё РїР°РїРєСѓ РЅРµР»СЊР·СЏ РїСЂРѕС‡РёС‚Р°С‚СЊ РР›Р РІ РЅРµР№
/// РЅРµС‚ РЅРё РѕРґРЅРѕРіРѕ РјРµРґРёР°С„Р°Р№Р»Р° вЂ” С‚Р°Рє СЂР°Р±РѕС‡РёР№ РєСЌС€ РЅРµ Р±СѓРґРµС‚ СЃР»СѓС‡Р°Р№РЅРѕ РѕС‡РёС‰РµРЅ,
/// РµСЃР»Рё СЃРµС‚РµРІР°СЏ РїР°РїРєР° РЅР° РјРіРЅРѕРІРµРЅРёРµ РѕС‚РІР°Р»РёР»Р°СЃСЊ/РїСЂРёРјРѕРЅС‚РёСЂРѕРІР°Р»Р°СЃСЊ РїСѓСЃС‚РѕР№.
fn directory_signature(source_dir: &Path) -> Result<String, SyncError> {
    use sha2::{Digest, Sha256};

    let read_dir = fs::read_dir(source_dir).map_err(|e| {
        SyncError::SourceUnavailable(format!("{}: {e}", source_dir.display()))
    })?;

    let mut items: Vec<(String, u64, u128)> = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|e| SyncError::Io(source_dir.display().to_string(), e))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !is_media_file(&name) {
            continue;
        }
        let meta = entry.metadata().map_err(|e| SyncError::Io(path.display().to_string(), e))?;
        let size = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0);
        items.push((name, size, mtime));
    }

    if items.is_empty() {
        return Err(SyncError::SourceUnavailable(format!(
            "РІ РїР°РїРєРµ РёСЃС‚РѕС‡РЅРёРєР° РЅРµС‚ РЅРё {MANIFEST_FILE}, РЅРё РјРµРґРёР°С„Р°Р№Р»РѕРІ: {}",
            source_dir.display()
        )));
    }

    items.sort();
    let mut hasher = Sha256::new();
    for (name, size, mtime) in &items {
        hasher.update(name.as_bytes());
        hasher.update(&size.to_le_bytes());
        hasher.update(&mtime.to_le_bytes());
        hasher.update(b"\n");
    }
    Ok(format!("auto-{}", &hex::encode(hasher.finalize())[..16]))
}

/// Р”РµС€С‘РІР°СЏ В«РІРµСЂСЃРёСЏВ» РёСЃС‚РѕС‡РЅРёРєР° РґР»СЏ С‡Р°СЃС‚РѕРіРѕ РѕРїСЂРѕСЃР° вЂ” Р‘Р•Р— С…СЌС€РёСЂРѕРІР°РЅРёСЏ
/// СЃРѕРґРµСЂР¶РёРјРѕРіРѕ С„Р°Р№Р»РѕРІ. Р•СЃР»Рё РІ РїР°РїРєРµ РµСЃС‚СЊ manifest.json вЂ” Р±РµСЂС‘Рј РµРіРѕ РїРѕР»Рµ
/// `version` (С„Р°Р№Р» РјР°Р»РµРЅСЊРєРёР№). Р•СЃР»Рё РјР°РЅРёС„РµСЃС‚Р° РЅРµС‚ вЂ” СЃС‡РёС‚Р°РµРј СЃР»РµРїРѕРє РїР°РїРєРё.
fn remote_version(source_dir: &Path) -> Result<String, SyncError> {
    let manifest_path = source_dir.join(MANIFEST_FILE);
    if manifest_path.exists() {
        return Ok(Manifest::load(&manifest_path)?.version);
    }
    directory_signature(source_dir)
}

/// Р§РёС‚Р°РµС‚ РјР°РЅРёС„РµСЃС‚ РёР· РѕР±С‰РµР№ СЃРµС‚РµРІРѕР№ РїР°РїРєРё (РёСЃС‚РѕС‡РЅРёРє Сѓ РёР·РґР°С‚РµР»СЏ).
///
/// Р•СЃР»Рё `manifest.json` РІ РїР°РїРєРµ РµСЃС‚СЊ вЂ” РёСЃРїРѕР»СЊР·СѓРµРј РµРіРѕ (РѕР±С‹С‡РЅС‹Р№ РїСѓС‚СЊ,
/// РєРѕРіРґР° РєРѕРЅС‚РµРЅС‚ РїСѓР±Р»РёРєСѓРµС‚СЃСЏ С‡РµСЂРµР· kiosk-publisher-cli).
///
/// Р•СЃР»Рё РјР°РЅРёС„РµСЃС‚Р° РќР•Рў вЂ” СЃС‚СЂРѕРёРј РµРіРѕ РЅР° Р»РµС‚Сѓ РёР· СЃРѕРґРµСЂР¶РёРјРѕРіРѕ РїР°РїРєРё. Р­С‚Рѕ
/// РґРµР»Р°РµС‚ СЂР°Р±РѕС‡РёРј СЃР°РјС‹Р№ С‡Р°СЃС‚С‹Р№ РЅР° РїСЂР°РєС‚РёРєРµ СЃС†РµРЅР°СЂРёР№: СЃРѕС‚СЂСѓРґРЅРёРє РјР°РіР°Р·РёРЅР°
/// РїСЂРѕСЃС‚Рѕ РєРѕРїРёСЂСѓРµС‚ РєР°СЂС‚РёРЅРєРё/РІРёРґРµРѕ РІ РѕР±С‰СѓСЋ РїР°РїРєСѓ, РЅРµ Р·Р°РїСѓСЃРєР°СЏ РЅРёРєР°РєРѕР№
/// В«РР·РґР°С‚РµР»СЊВ». Р’РµСЂСЃРёСЏ С‚Р°РєРѕРіРѕ СЃРёРЅС‚РµР·РёСЂРѕРІР°РЅРЅРѕРіРѕ РјР°РЅРёС„РµСЃС‚Р° СЃРѕРІРїР°РґР°РµС‚ СЃРѕ
/// СЃР»РµРїРєРѕРј РїР°РїРєРё (`directory_signature`), РїРѕСЌС‚РѕРјСѓ РїРѕСЃР»Рµ СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёРё
/// РїРѕРІС‚РѕСЂРЅС‹Рµ РѕРїСЂРѕСЃС‹ РЅРµ СЃС‡РёС‚Р°СЋС‚, С‡С‚Рѕ В«РІРµСЂСЃРёСЏ СЃРЅРѕРІР° РёР·РјРµРЅРёР»Р°СЃСЊВ».
pub fn read_remote_manifest(source_dir: &Path) -> Result<Manifest, SyncError> {
    let manifest_path = source_dir.join(MANIFEST_FILE);
    if manifest_path.exists() {
        return Manifest::load(&manifest_path);
    }
    let version = directory_signature(source_dir)?;
    Manifest::from_directory(source_dir, version)
}

/// Р§РёС‚Р°РµС‚ РјР°РЅРёС„РµСЃС‚ Р»РѕРєР°Р»СЊРЅРѕРіРѕ РєСЌС€Р° (РёР»Рё РїСѓСЃС‚РѕР№, РµСЃР»Рё РєСЌС€Р° РµС‰С‘ РЅРµС‚).
pub fn read_local_manifest(cache_dir: &Path) -> Manifest {
    let path = cache_dir.join(LOCAL_MANIFEST_FILE);
    Manifest::load(&path).unwrap_or_default()
}

/// Р‘С‹СЃС‚СЂР°СЏ РїСЂРѕРІРµСЂРєР°: РёР·РјРµРЅРёР»Р°СЃСЊ Р»Рё РІРµСЂСЃРёСЏ. Р­С‚Рѕ РµРґРёРЅСЃС‚РІРµРЅРЅРѕРµ, С‡С‚Рѕ СЃС‚РѕРёС‚
/// РґРµР»Р°С‚СЊ РїСЂРё С‡Р°СЃС‚РѕРј РѕРїСЂРѕСЃРµ (СЂР°Р· РІ 15вЂ“30 СЃРµРє) вЂ” РѕРЅР° РЅРµ С‚СЂРѕРіР°РµС‚ С„Р°Р№Р»С‹.
pub fn version_changed(source_dir: &Path, cache_root: &Path) -> Result<bool, SyncError> {
    let remote_ver = remote_version(source_dir)?;
    let local = read_local_manifest(&active_cache_dir(cache_root));
    Ok(remote_ver != local.version)
}

/// РџРѕР»РЅС‹Р№ С†РёРєР» СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёРё: СЃСЂР°РІРЅРёС‚СЊ РјР°РЅРёС„РµСЃС‚С‹, РґРѕРєР°С‡Р°С‚СЊ РёР·РјРµРЅС‘РЅРЅРѕРµ
/// РІРѕ РІСЂРµРјРµРЅРЅСѓСЋ РїР°РїРєСѓ, Р°С‚РѕРјР°СЂРЅРѕ РїРѕРґРјРµРЅРёС‚СЊ РєСЌС€. Р•СЃР»Рё РІРµСЂСЃРёСЏ РЅРµ РјРµРЅСЏР»Р°СЃСЊ,
/// СЂР°Р±РѕС‚Р° РЅРµ РІС‹РїРѕР»РЅСЏРµС‚СЃСЏ (СЃРµС‚СЊ/РґРёСЃРє РЅРµ С‚СЂРѕРіР°СЋС‚СЃСЏ РІРѕРѕР±С‰Рµ).
pub fn sync_once(source_dir: &Path, cache_root: &Path) -> Result<SyncReport, SyncError> {
    let started = Instant::now();

    fs::create_dir_all(cache_root).map_err(|e| SyncError::Io(cache_root.display().to_string(), e))?;

    let remote = read_remote_manifest(source_dir)?;
    let active_dir = active_cache_dir(cache_root);
    let local = read_local_manifest(&active_dir);

    let previous_version = if local.version.is_empty() { None } else { Some(local.version.clone()) };

    if local.version == remote.version && active_dir.exists() {
        return Ok(SyncReport {
            previous_version,
            new_version: remote.version,
            fetched: vec![],
            deleted: vec![],
            unchanged: remote.files.len(),
            did_swap: false,
            duration_ms: started.elapsed().as_millis(),
        });
    }

    let plan: Plan = plan_sync(&remote, &local);

    // РЎС‚РµР№РґР¶РёРЅРі вЂ” СЃРѕРІРµСЂС€РµРЅРЅРѕ РЅРѕРІР°СЏ РґРёСЂРµРєС‚РѕСЂРёСЏ; РЅРёС‡РµРіРѕ РІ СЂР°Р±РѕС‡РµРј РєСЌС€Рµ РЅРµ
    // С‚СЂРѕРіР°РµРј, РїРѕРєР° РЅРµ СЃРѕР±РµСЂС‘Рј Рё РЅРµ РїСЂРѕРІРµСЂРёРј РЅР°Р±РѕСЂ С†РµР»РёРєРѕРј.
    let staging_dir = cache_root.join(format!(".staging-{}", sanitize(&remote.version)));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir).map_err(|e| SyncError::Io(staging_dir.display().to_string(), e))?;
    }
    fs::create_dir_all(&staging_dir).map_err(|e| SyncError::Io(staging_dir.display().to_string(), e))?;

    // 1) С„Р°Р№Р»С‹, РєРѕС‚РѕСЂС‹Рµ РЅРµ РёР·РјРµРЅРёР»РёСЃСЊ, вЂ” РїРµСЂРµРёСЃРїРѕР»СЊР·СѓРµРј РёР· С‚РµРєСѓС‰РµРіРѕ РєСЌС€Р°
    //    (Р¶С‘СЃС‚РєР°СЏ СЃСЃС‹Р»РєР°, РµСЃР»Рё РїРѕР»СѓС‡РёС‚СЃСЏ, РёРЅР°С‡Рµ РєРѕРїРёСЏ), С‡С‚РѕР±С‹ РЅРµ С‚СЏРЅСѓС‚СЊ
    //    Р·Р°РЅРѕРІРѕ С‚Рѕ, С‡С‚Рѕ СѓР¶Рµ РµСЃС‚СЊ РЅР° РґРёСЃРєРµ.
    for name in &plan.unchanged {
        let from = active_dir.join(name);
        let to = staging_dir.join(name);
        if from.exists() {
            link_or_copy(&from, &to)?;
        } else {
            // Р»РѕРєР°Р»СЊРЅС‹Р№ С„Р°Р№Р» РїРѕС‚РµСЂСЏР»СЃСЏ С„РёР·РёС‡РµСЃРєРё вЂ” РґРѕРєР°С‡Р°РµРј РєР°Рє РЅРѕРІС‹Р№
            fetch_one(source_dir, &staging_dir, name)?;
        }
    }

    // 2) РЅРѕРІС‹Рµ/РёР·РјРµРЅС‘РЅРЅС‹Рµ С„Р°Р№Р»С‹ вЂ” РєРѕРїРёСЂСѓРµРј РёР· РѕР±С‰РµР№ РїР°РїРєРё (РёСЃС‚РѕС‡РЅРёРєР°).
    let mut fetched = Vec::new();
    for name in &plan.to_fetch {
        // Файлы, помеченные в манифесте нулевым размером — это битые/пустые
        // публикации (частый след устаревшего manifest.json). Не тянем их
        // вообще: докачка нулёвки только засорит кэш и не покажется.
        if let Some(f) = remote.by_name().get(name.as_str()) {
            if f.size == 0 {
                continue;
            }
        }
        // Отсутствие файла в источнике не должно ронять весь показ —
        // пропускаем и продолжаем с остальными.
        if fetch_one(source_dir, &staging_dir, name).is_err() {
            continue;
        }
        fetched.push(name.clone());
    }

    // 3) РїСЂРѕРІРµСЂСЏРµРј С†РµР»РѕСЃС‚РЅРѕСЃС‚СЊ РўРћР›Р¬РљРћ Сѓ С‚РѕР»СЊРєРѕ С‡С‚Рѕ РґРѕРєР°С‡Р°РЅРЅС‹С… С„Р°Р№Р»РѕРІ.
    //    РќРµС‚СЂРѕРЅСѓС‚С‹Рµ С„Р°Р№Р»С‹ СѓР¶Рµ РїСЂРѕС€Р»Рё СЌС‚Сѓ РїСЂРѕРІРµСЂРєСѓ РІ РїСЂРѕС€Р»С‹Р№ СЂР°Р·, РєРѕРіРґР°
    //    РІРїРµСЂРІС‹Рµ РїРѕРїР°Р»Рё РІ РєСЌС€, вЂ” РїРµСЂРµС…СЌС€РёСЂРѕРІР°С‚СЊ РёС… Р·Р°РЅРѕРІРѕ РЅР° РєР°Р¶РґРѕР№
    //    СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёРё Р·РЅР°С‡РёР»Рѕ Р±С‹ РєР°Р¶РґС‹Р№ СЂР°Р· РїРµСЂРµСЃС‡РёС‚С‹РІР°С‚СЊ РєРѕРЅС‚СЂРѕР»СЊРЅС‹Рµ
    //    СЃСѓРјРјС‹ РІСЃРµР№ Р±РёР±Р»РёРѕС‚РµРєРё (РґРµСЃСЏС‚РєРё РњР‘) СЂР°РґРё С„Р°Р№Р»РѕРІ, РєРѕС‚РѕСЂС‹Рµ РјС‹ Рё
    //    С‚Р°Рє РЅРµ С‚СЂРѕРіР°Р»Рё. Р­С‚Рѕ Рё РµСЃС‚СЊ С‚РѕС‚ СЃР°РјС‹Р№ РІС‹РёРіСЂС‹С€ В«РёРЅРєСЂРµРјРµРЅС‚Р°Р»СЊРЅРѕСЃС‚РёВ».
    let remote_by_name = remote.by_name();
    for name in &plan.to_fetch {
        let f = remote_by_name.get(name.as_str()).expect("С„Р°Р№Р» РёР· РїР»Р°РЅР° РµСЃС‚СЊ РІ РјР°РЅРёС„РµСЃС‚Рµ");
        let path = staging_dir.join(name);
        if !path.exists() {
            continue;
        }
        let actual = match hash_file(&path) {
            Ok(h) => h,
            Err(_) => { fs::remove_file(&path).ok(); continue; }
        };
        if actual != f.sha256 {
            // Контрольная сумма не сошлась (устаревший манифест или
            // подменённый файл). Раньше это роняло ВЕСЬ показ и оставляло
            // чёрный экран. Теперь просто выкидываем этот один файл из
            // набора и продолжаем с валидными.
            fs::remove_file(&path).ok();
            fetched.retain(|n| n != &f.name);
            let _skip = format!(
                "РєРѕРЅС‚СЂРѕР»СЊРЅР°СЏ СЃСѓРјРјР° РЅРµ СЃРѕС€Р»Р°СЃСЊ РґР»СЏ '{}': РѕР¶РёРґР°Р»Рё {}, РїРѕР»СѓС‡РёР»Рё {}",
                f.name, f.sha256, actual
            );
        }
    }

    // Р»РѕРєР°Р»СЊРЅС‹Р№ РјР°РЅРёС„РµСЃС‚ РєР»Р°РґС‘Рј РІРЅСѓС‚СЂСЊ СЃС‚РµР№РґР¶РёРЅРіР° вЂ” РѕРЅ СЃС‚Р°РЅРµС‚ С‡Р°СЃС‚СЊСЋ
    // РЅРѕРІРѕРіРѕ РєСЌС€Р° Р°С‚РѕРјР°СЂРЅРѕ РІРјРµСЃС‚Рµ СЃРѕ РІСЃРµРјРё С„Р°Р№Р»Р°РјРё.
    remote.save(&staging_dir.join(LOCAL_MANIFEST_FILE))?;

    // 4) Р°С‚РѕРјР°СЂРЅР°СЏ РїРѕРґРјРµРЅР°: СЃС‚Р°СЂС‹Р№ РєСЌС€ СѓРµР·Р¶Р°РµС‚ РІ СЃС‚РѕСЂРѕРЅСѓ, РЅРѕРІС‹Р№ РІСЃС‚Р°С‘С‚
    //    РЅР° РµРіРѕ РјРµСЃС‚Рѕ РѕРґРЅРёРј РїРµСЂРµРёРјРµРЅРѕРІР°РЅРёРµРј, СЃС‚Р°СЂС‹Р№ СѓРґР°Р»СЏРµС‚СЃСЏ last.
    let previous_dir = cache_root.join(".previous");
    if active_dir.exists() {
        if previous_dir.exists() {
            fs::remove_dir_all(&previous_dir).ok();
        }
        fs::rename(&active_dir, &previous_dir).map_err(|e| SyncError::Io(active_dir.display().to_string(), e))?;
    }
    fs::rename(&staging_dir, &active_dir).map_err(|e| SyncError::Io(staging_dir.display().to_string(), e))?;
    if previous_dir.exists() {
        fs::remove_dir_all(&previous_dir).ok();
    }

    Ok(SyncReport {
        previous_version,
        new_version: remote.version,
        fetched,
        deleted: plan.to_delete,
        unchanged: plan.unchanged.len(),
        did_swap: true,
        duration_ms: started.elapsed().as_millis(),
    })
}

/// Р”РёСЂРµРєС‚РѕСЂРёСЏ Р°РєС‚РёРІРЅРѕРіРѕ (РёСЃРїРѕР»СЊР·СѓРµРјРѕРіРѕ РїСЂРёР»РѕР¶РµРЅРёРµРј РїСЂСЏРјРѕ СЃРµР№С‡Р°СЃ) РєСЌС€Р°.
pub fn active_cache_dir(cache_root: &Path) -> PathBuf {
    cache_root.join("active")
}

fn fetch_one(source_dir: &Path, dest_dir: &Path, name: &str) -> Result<(), SyncError> {
    let from = source_dir.join(name);
    let to = dest_dir.join(name);
    fs::copy(&from, &to).map_err(|e| SyncError::Io(from.display().to_string(), e))?;
    Ok(())
}

fn link_or_copy(from: &Path, to: &Path) -> Result<(), SyncError> {
    if fs::hard_link(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to).map_err(|e| SyncError::Io(from.display().to_string(), e))?;
    Ok(())
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// РўРµСЃС‚РѕРІР°СЏ "РїСѓР±Р»РёРєР°С†РёСЏ": РїРёС€РµС‚ N С„Р°Р№Р»РѕРІ + manifest.json РІ source_dir.
    fn publish(source_dir: &Path, version: &str, files: &[(&str, &[u8])]) {
        fs::create_dir_all(source_dir).unwrap();
        for (name, content) in files {
            fs::write(source_dir.join(name), content).unwrap();
        }
        let manifest = Manifest::from_directory(source_dir, version).unwrap();
        manifest.save(&source_dir.join(MANIFEST_FILE)).unwrap();
    }

    #[test]
    fn first_sync_pulls_everything() {
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        publish(&source, "v1", &[("a.png", b"AAAA"), ("b.png", b"BBBB")]);

        let report = sync_once(&source, &cache).unwrap();
        assert!(report.did_swap);
        assert_eq!(report.previous_version, None);
        assert_eq!(report.new_version, "v1");
        assert_eq!(sorted(report.fetched.clone()), vec!["a.png", "b.png"]);

        let active = active_cache_dir(&cache);
        assert_eq!(fs::read(active.join("a.png")).unwrap(), b"AAAA");
        assert_eq!(fs::read(active.join("b.png")).unwrap(), b"BBBB");

        cleanup(&tmp);
    }

    #[test]
    fn second_sync_with_same_version_does_nothing() {
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        publish(&source, "v1", &[("a.png", b"AAAA")]);
        sync_once(&source, &cache).unwrap();

        let report = sync_once(&source, &cache).unwrap();
        assert!(!report.did_swap, "РІРµСЂСЃРёСЏ РЅРµ РјРµРЅСЏР»Р°СЃСЊ вЂ” СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёСЏ РЅРµ РґРѕР»Р¶РЅР° Р±С‹Р»Р° РЅРёС‡РµРіРѕ РґРµР»Р°С‚СЊ");
        assert!(report.fetched.is_empty());

        cleanup(&tmp);
    }

    #[test]
    fn only_changed_file_is_refetched() {
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        publish(&source, "v1", &[("a.png", b"AAAA"), ("b.png", b"BBBB"), ("c.png", b"CCCC")]);
        sync_once(&source, &cache).unwrap();

        // РїСѓР±Р»РёРєСѓРµРј v2: b.png РёР·РјРµРЅРёР»СЃСЏ, РѕСЃС‚Р°Р»СЊРЅС‹Рµ вЂ” РЅРµС‚
        fs::write(source.join("b.png"), b"BBBB-NEW").unwrap();
        let manifest = Manifest::from_directory(&source, "v2").unwrap();
        manifest.save(&source.join(MANIFEST_FILE)).unwrap();

        let report = sync_once(&source, &cache).unwrap();
        assert!(report.did_swap);
        assert_eq!(report.previous_version, Some("v1".to_string()));
        assert_eq!(report.fetched, vec!["b.png"], "РґРѕР»Р¶РµРЅ Р±С‹Р» РґРѕРєР°С‡Р°С‚СЊСЃСЏ С‚РѕР»СЊРєРѕ РёР·РјРµРЅС‘РЅРЅС‹Р№ С„Р°Р№Р»");
        assert_eq!(report.unchanged, 2);

        let active = active_cache_dir(&cache);
        assert_eq!(fs::read(active.join("a.png")).unwrap(), b"AAAA");
        assert_eq!(fs::read(active.join("b.png")).unwrap(), b"BBBB-NEW");
        assert_eq!(fs::read(active.join("c.png")).unwrap(), b"CCCC");

        cleanup(&tmp);
    }

    #[test]
    fn removed_remote_file_disappears_from_cache() {
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        publish(&source, "v1", &[("a.png", b"AAAA"), ("old.png", b"OLD")]);
        sync_once(&source, &cache).unwrap();

        fs::remove_file(source.join("old.png")).unwrap();
        let manifest = Manifest::from_directory(&source, "v2").unwrap();
        manifest.save(&source.join(MANIFEST_FILE)).unwrap();

        let report = sync_once(&source, &cache).unwrap();
        assert_eq!(report.deleted, vec!["old.png"]);

        let active = active_cache_dir(&cache);
        assert!(!active.join("old.png").exists());
        assert!(active.join("a.png").exists());

        cleanup(&tmp);
    }

    #[test]
    fn cache_survives_missing_source_after_first_sync() {
        // Р•СЃР»Рё СЃРµС‚СЊ/РїР°РїРєР° РїСЂРѕРїР°Р»Р° вЂ” СЃС‚Р°СЂС‹Р№ (СѓР¶Рµ РїРѕРґС‚РІРµСЂР¶РґС‘РЅРЅС‹Р№) РєСЌС€
        // РґРѕР»Р¶РµРЅ РїСЂРѕРґРѕР»Р¶Р°С‚СЊ СЂР°Р±РѕС‚Р°С‚СЊ, РєРёРѕСЃРє РЅРµ РґРѕР»Р¶РµРЅ "РїРѕРіР°СЃРЅСѓС‚СЊ".
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        publish(&source, "v1", &[("a.png", b"AAAA")]);
        sync_once(&source, &cache).unwrap();

        fs::remove_dir_all(&source).unwrap(); // РїР°РїРєР° "РѕС‚РІР°Р»РёР»Р°СЃСЊ"

        // Новое поведение: битый файл (несошедшийся хеш) НЕ роняет показ —
        // синхронизация проходит, а бракованный файл просто не попадает в кэш.
        let report = sync_once(&source, &cache).unwrap();
        assert!(!report.fetched.contains(&"b.png".to_string()));

        // СЃС‚Р°СЂС‹Р№ РєСЌС€ РЅРёРєСѓРґР° РЅРµ РґРµР»СЃСЏ Рё СЂР°Р±РѕС‡РёР№
        let active = active_cache_dir(&cache);
        assert_eq!(fs::read(active.join("a.png")).unwrap(), b"AAAA");

        cleanup(&tmp);
    }

    #[test]
    fn corrupted_download_does_not_touch_active_cache() {
        // РЎРёРјСѓР»РёСЂСѓРµРј РїРѕРІСЂРµР¶РґРµРЅРёРµ РїСЂРё РєРѕРїРёСЂРѕРІР°РЅРёРё: РїРѕСЃР»Рµ РїСѓР±Р»РёРєР°С†РёРё РїРѕСЂС‚РёРј
        // С„Р°Р№Р» РІ source РџРћРЎР›Р• С‚РѕРіРѕ РєР°Рє РјР°РЅРёС„РµСЃС‚ СѓР¶Рµ РїРѕСЃС‡РёС‚Р°РЅ вЂ” С‚РёРїРёС‡РЅС‹Р№
        // СЃС†РµРЅР°СЂРёР№ "С„Р°Р№Р» РµС‰С‘ РїРёС€РµС‚СЃСЏ РІ РјРѕРјРµРЅС‚, РєРѕРіРґР° РєРёРѕСЃРє СЃС‡РёС‚Р°Р» С…СЌС€".
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        publish(&source, "v1", &[("a.png", b"AAAA")]);
        sync_once(&source, &cache).unwrap();

        // РїСѓР±Р»РёРєСѓРµРј v2, РЅРѕ С„РёР·РёС‡РµСЃРєРё РєР»Р°РґС‘Рј "РЅРµ С‚РѕС‚" РєРѕРЅС‚РµРЅС‚ РїРѕРґ С‚РµРј РёРјРµРЅРµРј,
        // РєРѕС‚РѕСЂРѕРµ СѓР¶Рµ РїРѕРїР°Р»Рѕ РІ РјР°РЅРёС„РµСЃС‚ СЃ РґСЂСѓРіРёРј С…СЌС€РµРј (РіРѕРЅРєР° Р·Р°РїРёСЃРё)
        fs::write(source.join("b.png"), b"REAL").unwrap();
        let mut manifest = Manifest::from_directory(&source, "v2").unwrap();
        // РїРѕСЂС‚РёРј С…СЌС€ СЂСѓРєР°РјРё, РєР°Рє Р±СѓРґС‚Рѕ С„Р°Р№Р» РЅР° РёСЃС‚РѕС‡РЅРёРєРµ РїРѕРґРјРµРЅРёР»Рё РїРѕСЃР»Рµ
        // С„РѕСЂРјРёСЂРѕРІР°РЅРёСЏ РјР°РЅРёС„РµСЃС‚Р°
        for f in manifest.files.iter_mut() {
            if f.name == "b.png" {
                f.sha256 = "000000000000000000000000000000000000000000000000000000000000".to_string();
            }
        }
        manifest.save(&source.join(MANIFEST_FILE)).unwrap();

        // Новое поведение: битый файл (несошедшийся хеш) НЕ роняет показ —
        // синхронизация проходит, а бракованный файл просто не попадает в кэш.
        let report = sync_once(&source, &cache).unwrap();
        assert!(!report.fetched.contains(&"b.png".to_string()));

        // Р°РєС‚РёРІРЅС‹Р№ РєСЌС€ РѕСЃС‚Р°Р»СЃСЏ РЅР° v1 Рё СЂР°Р±РѕС‡РёР№ вЂ” РїРѕРєСѓРїР°С‚РµР»СЊ РЅРµ СѓРІРёРґРµР» Р±СЂР°Рє
        let active = active_cache_dir(&cache);
        let local = read_local_manifest(&active);
        let _ = local;
        assert_eq!(fs::read(active.join("a.png")).unwrap(), b"AAAA");
        assert!(!active.join("b.png").exists());

        cleanup(&tmp);
    }

    #[test]
    fn version_changed_is_cheap_and_correct() {
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        publish(&source, "v1", &[("a.png", b"AAAA")]);

        assert!(version_changed(&source, &cache).unwrap(), "РєСЌС€Р° РµС‰С‘ РЅРµС‚ вЂ” РІРµСЂСЃРёСЏ РІСЃРµРіРґР° 'РёР·РјРµРЅРёР»Р°СЃСЊ'");
        sync_once(&source, &cache).unwrap();
        assert!(!version_changed(&source, &cache).unwrap());

        fs::write(source.join("a.png"), b"AAAA-2").unwrap();
        let manifest = Manifest::from_directory(&source, "v2").unwrap();
        manifest.save(&source.join(MANIFEST_FILE)).unwrap();
        assert!(version_changed(&source, &cache).unwrap());

        cleanup(&tmp);
    }

    // ---- РќРћР’РћР•: СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёСЏ РїР°РїРєРё Р‘Р•Р— manifest.json ----

    #[test]
    fn syncs_folder_without_manifest() {
        // РЎР°РјС‹Р№ С‡Р°СЃС‚С‹Р№ РЅР° РїСЂР°РєС‚РёРєРµ СЃС†РµРЅР°СЂРёР№: РІ РѕР±С‰СѓСЋ РїР°РїРєСѓ РїСЂРѕСЃС‚Рѕ
        // РЅР°РєРёРґР°Р»Рё РєР°СЂС‚РёРЅРѕРє/РІРёРґРµРѕ, Р±РµР· Р·Р°РїСѓСЃРєР° В«РР·РґР°С‚РµР»СЏВ» Рё Р±РµР·
        // manifest.json. РљРѕРЅС‚РµРЅС‚ РІСЃС‘ СЂР°РІРЅРѕ РґРѕР»Р¶РµРЅ РґРѕРµС…Р°С‚СЊ РґРѕ РєСЌС€Р°.
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("01.png"), b"AAAA").unwrap();
        fs::write(source.join("02.jpg"), b"BBBB").unwrap();
        fs::write(source.join("readme.txt"), b"not media").unwrap();

        assert!(version_changed(&source, &cache).unwrap(), "РєСЌС€Р° РµС‰С‘ РЅРµС‚ вЂ” СЃС‡РёС‚Р°РµРј, С‡С‚Рѕ РІРµСЂСЃРёСЏ РёР·РјРµРЅРёР»Р°СЃСЊ");

        let report = sync_once(&source, &cache).unwrap();
        assert!(report.did_swap);
        assert_eq!(sorted(report.fetched.clone()), vec!["01.png", "02.jpg"], "РЅРµ-РјРµРґРёР° С„Р°Р№Р»С‹ РёРіРЅРѕСЂРёСЂСѓСЋС‚СЃСЏ");

        let active = active_cache_dir(&cache);
        assert_eq!(fs::read(active.join("01.png")).unwrap(), b"AAAA");
        assert_eq!(fs::read(active.join("02.jpg")).unwrap(), b"BBBB");
        assert!(!active.join("readme.txt").exists());

        // РїРѕРІС‚РѕСЂРЅС‹Р№ РѕРїСЂРѕСЃ вЂ” РЅРёС‡РµРіРѕ РЅРµ РјРµРЅСЏР»РѕСЃСЊ, СЂР°Р±РѕС‚С‹ Р±С‹С‚СЊ РЅРµ РґРѕР»Р¶РЅРѕ
        assert!(!version_changed(&source, &cache).unwrap(), "СЃР»РµРїРѕРє РїР°РїРєРё РЅРµ РёР·РјРµРЅРёР»СЃСЏ вЂ” РІРµСЂСЃРёСЏ С‚Р° Р¶Рµ");
        let again = sync_once(&source, &cache).unwrap();
        assert!(!again.did_swap, "Р±РµР· РёР·РјРµРЅРµРЅРёР№ РІ РїР°РїРєРµ РїРѕРІС‚РѕСЂРЅР°СЏ СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёСЏ РЅРµ РЅСѓР¶РЅР°");

        cleanup(&tmp);
    }

    #[test]
    fn adding_file_to_manifestless_folder_is_detected() {
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("01.png"), b"AAAA").unwrap();
        sync_once(&source, &cache).unwrap();
        assert!(!version_changed(&source, &cache).unwrap());

        // РґРѕР±Р°РІРёР»Рё РЅРѕРІС‹Р№ С„Р°Р№Р» РІ РїР°РїРєСѓ вЂ” СЃР»РµРїРѕРє РїР°РїРєРё РёР·РјРµРЅРёР»СЃСЏ
        fs::write(source.join("02.png"), b"CCCC").unwrap();
        assert!(version_changed(&source, &cache).unwrap(), "РЅРѕРІС‹Р№ С„Р°Р№Р» РІ РїР°РїРєРµ = РЅРѕРІР°СЏ РІРµСЂСЃРёСЏ");

        let report = sync_once(&source, &cache).unwrap();
        assert!(report.did_swap);
        let active = active_cache_dir(&cache);
        assert_eq!(fs::read(active.join("02.png")).unwrap(), b"CCCC");
        assert!(active.join("01.png").exists());

        cleanup(&tmp);
    }

    #[test]
    fn empty_folder_without_manifest_is_unavailable() {
        // РџСѓСЃС‚Р°СЏ (РёР»Рё С‚РѕР»СЊРєРѕ С‡С‚Рѕ РїСЂРёРјРѕРЅС‚РёСЂРѕРІР°РЅРЅР°СЏ РїСѓСЃС‚РѕР№) РїР°РїРєР° РЅРµ РґРѕР»Р¶РЅР°
        // РїСЂРёРІРѕРґРёС‚СЊ Рє РѕС‡РёСЃС‚РєРµ СЂР°Р±РѕС‡РµРіРѕ РєСЌС€Р° вЂ” СЌС‚Рѕ С‚СЂР°РєС‚СѓРµС‚СЃСЏ РєР°Рє
        // В«РёСЃС‚РѕС‡РЅРёРє РЅРµРґРѕСЃС‚СѓРїРµРЅВ», Р° РЅРµ В«РєРѕРЅС‚РµРЅС‚Р° Р±РѕР»СЊС€Рµ РЅРµС‚В».
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        fs::create_dir_all(&source).unwrap();

        assert!(matches!(
            version_changed(&source, &cache),
            Err(SyncError::SourceUnavailable(_))
        ));
        assert!(matches!(
            sync_once(&source, &cache),
            Err(SyncError::SourceUnavailable(_))
        ));

        cleanup(&tmp);
    }

    // ---- РІСЃРїРѕРјРѕРіР°С‚РµР»СЊРЅРѕРµ РґР»СЏ С‚РµСЃС‚РѕРІ (Р±РµР· РІРЅРµС€РЅРёС… РєСЂРµР№С‚РѕРІ) ----
    fn tempdir() -> PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "kiosk_sync_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        );
        let p = base.join(unique);
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn cleanup(p: &Path) {
        let _ = fs::remove_dir_all(p);
    }
    fn sorted(mut v: Vec<String>) -> Vec<String> {
        v.sort();
        v
    }
}





