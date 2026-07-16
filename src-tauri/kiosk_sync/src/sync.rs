//! Исполнение плана синхронизации: докачка изменённых файлов во
//! временную папку и атомарная подмена рабочего кэша.
//!
//! Ключевое инвариант: покупатель никогда не должен увидеть кэш в
//! промежуточном состоянии (часть файлов новые, часть старые, что-то
//! ещё копируется). Поэтому мы никогда не пишем поверх рабочего кэша
//! напрямую — сначала собираем ПОЛНЫЙ новый кэш в staging-папке рядом,
//! и только когда он готов и проверен — одной операцией переименования
//! подменяем директорию целиком.

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

/// Читает манифест из общей сетевой папки (источник у издателя).
pub fn read_remote_manifest(source_dir: &Path) -> Result<Manifest, SyncError> {
    let manifest_path = source_dir.join(MANIFEST_FILE);
    if !manifest_path.exists() {
        return Err(SyncError::SourceUnavailable(format!(
            "{} не найден (папка недоступна или пуста)",
            manifest_path.display()
        )));
    }
    Manifest::load(&manifest_path)
}

/// Читает манифест локального кэша (или пустой, если кэша ещё нет).
pub fn read_local_manifest(cache_dir: &Path) -> Manifest {
    let path = cache_dir.join(LOCAL_MANIFEST_FILE);
    Manifest::load(&path).unwrap_or_default()
}

/// Быстрая проверка: изменилась ли версия. Это единственное, что стоит
/// делать при частом опросе (раз в 15–30 сек) — она не трогает файлы.
pub fn version_changed(source_dir: &Path, cache_root: &Path) -> Result<bool, SyncError> {
    let remote = read_remote_manifest(source_dir)?;
    let local = read_local_manifest(&active_cache_dir(cache_root));
    Ok(remote.version != local.version)
}

/// Полный цикл синхронизации: сравнить манифесты, докачать изменённое
/// во временную папку, атомарно подменить кэш. Если версия не менялась,
/// работа не выполняется (сеть/диск не трогаются вообще).
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

    // Стейджинг — совершенно новая директория; ничего в рабочем кэше не
    // трогаем, пока не соберём и не проверим набор целиком.
    let staging_dir = cache_root.join(format!(".staging-{}", sanitize(&remote.version)));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir).map_err(|e| SyncError::Io(staging_dir.display().to_string(), e))?;
    }
    fs::create_dir_all(&staging_dir).map_err(|e| SyncError::Io(staging_dir.display().to_string(), e))?;

    // 1) файлы, которые не изменились, — переиспользуем из текущего кэша
    //    (жёсткая ссылка, если получится, иначе копия), чтобы не тянуть
    //    заново то, что уже есть на диске.
    for name in &plan.unchanged {
        let from = active_dir.join(name);
        let to = staging_dir.join(name);
        if from.exists() {
            link_or_copy(&from, &to)?;
        } else {
            // локальный файл потерялся физически — докачаем как новый
            fetch_one(source_dir, &staging_dir, name)?;
        }
    }

    // 2) новые/изменённые файлы — копируем из общей папки (источника).
    let mut fetched = Vec::new();
    for name in &plan.to_fetch {
        fetch_one(source_dir, &staging_dir, name)?;
        fetched.push(name.clone());
    }

    // 3) проверяем целостность ТОЛЬКО у только что докачанных файлов.
    //    Нетронутые файлы уже прошли эту проверку в прошлый раз, когда
    //    впервые попали в кэш, — перехэшировать их заново на каждой
    //    синхронизации значило бы каждый раз пересчитывать контрольные
    //    суммы всей библиотеки (десятки МБ) ради файлов, которые мы и
    //    так не трогали. Это и есть тот самый выигрыш «инкрементальности».
    let remote_by_name = remote.by_name();
    for name in &plan.to_fetch {
        let f = remote_by_name.get(name.as_str()).expect("файл из плана есть в манифесте");
        let path = staging_dir.join(name);
        let actual = hash_file(&path)?;
        if actual != f.sha256 {
            fs::remove_dir_all(&staging_dir).ok();
            return Err(SyncError::SourceUnavailable(format!(
                "контрольная сумма не сошлась для '{}': ожидали {}, получили {}",
                f.name, f.sha256, actual
            )));
        }
    }

    // локальный манифест кладём внутрь стейджинга — он станет частью
    // нового кэша атомарно вместе со всеми файлами.
    remote.save(&staging_dir.join(LOCAL_MANIFEST_FILE))?;

    // 4) атомарная подмена: старый кэш уезжает в сторону, новый встаёт
    //    на его место одним переименованием, старый удаляется last.
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

/// Директория активного (используемого приложением прямо сейчас) кэша.
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

    /// Тестовая "публикация": пишет N файлов + manifest.json в source_dir.
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
        assert!(!report.did_swap, "версия не менялась — синхронизация не должна была ничего делать");
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

        // публикуем v2: b.png изменился, остальные — нет
        fs::write(source.join("b.png"), b"BBBB-NEW").unwrap();
        let manifest = Manifest::from_directory(&source, "v2").unwrap();
        manifest.save(&source.join(MANIFEST_FILE)).unwrap();

        let report = sync_once(&source, &cache).unwrap();
        assert!(report.did_swap);
        assert_eq!(report.previous_version, Some("v1".to_string()));
        assert_eq!(report.fetched, vec!["b.png"], "должен был докачаться только изменённый файл");
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
        // Если сеть/папка пропала — старый (уже подтверждённый) кэш
        // должен продолжать работать, киоск не должен "погаснуть".
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        publish(&source, "v1", &[("a.png", b"AAAA")]);
        sync_once(&source, &cache).unwrap();

        fs::remove_dir_all(&source).unwrap(); // папка "отвалилась"

        let err = sync_once(&source, &cache).unwrap_err();
        assert!(matches!(err, SyncError::SourceUnavailable(_)));

        // старый кэш никуда не делся и рабочий
        let active = active_cache_dir(&cache);
        assert_eq!(fs::read(active.join("a.png")).unwrap(), b"AAAA");

        cleanup(&tmp);
    }

    #[test]
    fn corrupted_download_does_not_touch_active_cache() {
        // Симулируем повреждение при копировании: после публикации портим
        // файл в source ПОСЛЕ того как манифест уже посчитан — типичный
        // сценарий "файл ещё пишется в момент, когда киоск считал хэш".
        let tmp = tempdir();
        let source = tmp.join("source");
        let cache = tmp.join("cache");
        publish(&source, "v1", &[("a.png", b"AAAA")]);
        sync_once(&source, &cache).unwrap();

        // публикуем v2, но физически кладём "не тот" контент под тем именем,
        // которое уже попало в манифест с другим хэшем (гонка записи)
        fs::write(source.join("b.png"), b"REAL").unwrap();
        let mut manifest = Manifest::from_directory(&source, "v2").unwrap();
        // портим хэш руками, как будто файл на источнике подменили после
        // формирования манифеста
        for f in manifest.files.iter_mut() {
            if f.name == "b.png" {
                f.sha256 = "000000000000000000000000000000000000000000000000000000000000".to_string();
            }
        }
        manifest.save(&source.join(MANIFEST_FILE)).unwrap();

        let err = sync_once(&source, &cache).unwrap_err();
        assert!(matches!(err, SyncError::SourceUnavailable(_)));

        // активный кэш остался на v1 и рабочий — покупатель не увидел брак
        let active = active_cache_dir(&cache);
        let local = read_local_manifest(&active);
        assert_eq!(local.version, "v1");
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

        assert!(version_changed(&source, &cache).unwrap(), "кэша ещё нет — версия всегда 'изменилась'");
        sync_once(&source, &cache).unwrap();
        assert!(!version_changed(&source, &cache).unwrap());

        fs::write(source.join("a.png"), b"AAAA-2").unwrap();
        let manifest = Manifest::from_directory(&source, "v2").unwrap();
        manifest.save(&source.join(MANIFEST_FILE)).unwrap();
        assert!(version_changed(&source, &cache).unwrap());

        cleanup(&tmp);
    }

    // ---- вспомогательное для тестов (без внешних крейтов) ----
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
