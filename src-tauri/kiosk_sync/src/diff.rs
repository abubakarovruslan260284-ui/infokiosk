//! Сравнение двух манифестов: что нужно скачать, что удалить.
//!
//! Сравнение идёт по имени файла и его SHA-256. Если хэш совпал — файл
//! не трогаем, даже если у него другой `order` или дата изменения:
//! содержимое то же самое, докачивать нечего.

use crate::manifest::Manifest;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// Файлы, которых нет локально или которые изменились — их нужно скачать.
    pub to_fetch: Vec<String>,
    /// Файлы, которые есть локально, но пропали из удалённого манифеста.
    pub to_delete: Vec<String>,
    /// Файлы, которые не изменились — трогать не нужно.
    pub unchanged: Vec<String>,
}

pub fn plan_sync(remote: &Manifest, local: &Manifest) -> Plan {
    let local_by_name = local.by_name();
    let remote_by_name = remote.by_name();

    let mut plan = Plan::default();

    for f in &remote.files {
        match local_by_name.get(f.name.as_str()) {
            Some(local_entry) if local_entry.sha256 == f.sha256 => {
                plan.unchanged.push(f.name.clone());
            }
            _ => {
                plan.to_fetch.push(f.name.clone());
            }
        }
    }

    for f in &local.files {
        if !remote_by_name.contains_key(f.name.as_str()) {
            plan.to_delete.push(f.name.clone());
        }
    }

    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::FileEntry;

    fn entry(name: &str, hash: &str) -> FileEntry {
        FileEntry { name: name.into(), sha256: hash.into(), size: 10, order: None }
    }

    #[test]
    fn empty_local_fetches_everything() {
        let remote = Manifest { version: "1".into(), files: vec![entry("a.png", "h1"), entry("b.png", "h2")] };
        let local = Manifest::default();
        let plan = plan_sync(&remote, &local);
        assert_eq!(plan.to_fetch, vec!["a.png", "b.png"]);
        assert!(plan.to_delete.is_empty());
        assert!(plan.unchanged.is_empty());
    }

    #[test]
    fn identical_manifests_fetch_nothing() {
        let m = Manifest { version: "1".into(), files: vec![entry("a.png", "h1")] };
        let plan = plan_sync(&m, &m);
        assert!(plan.to_fetch.is_empty());
        assert!(plan.to_delete.is_empty());
        assert_eq!(plan.unchanged, vec!["a.png"]);
    }

    #[test]
    fn changed_hash_triggers_refetch_of_only_that_file() {
        let remote = Manifest {
            version: "2".into(),
            files: vec![entry("a.png", "h1"), entry("b.png", "CHANGED")],
        };
        let local = Manifest {
            version: "1".into(),
            files: vec![entry("a.png", "h1"), entry("b.png", "h2")],
        };
        let plan = plan_sync(&remote, &local);
        assert_eq!(plan.to_fetch, vec!["b.png"]);
        assert_eq!(plan.unchanged, vec!["a.png"]);
        assert!(plan.to_delete.is_empty());
    }

    #[test]
    fn removed_remote_file_is_deleted_locally() {
        let remote = Manifest { version: "2".into(), files: vec![entry("a.png", "h1")] };
        let local = Manifest {
            version: "1".into(),
            files: vec![entry("a.png", "h1"), entry("old.png", "h9")],
        };
        let plan = plan_sync(&remote, &local);
        assert!(plan.to_fetch.is_empty());
        assert_eq!(plan.to_delete, vec!["old.png"]);
        assert_eq!(plan.unchanged, vec!["a.png"]);
    }

    #[test]
    fn rename_is_fetch_plus_delete() {
        // Переименование = с точки зрения манифеста новый файл + пропажа старого.
        let remote = Manifest { version: "2".into(), files: vec![entry("b_new.png", "h1")] };
        let local = Manifest { version: "1".into(), files: vec![entry("a_old.png", "h1")] };
        let plan = plan_sync(&remote, &local);
        assert_eq!(plan.to_fetch, vec!["b_new.png"]);
        assert_eq!(plan.to_delete, vec!["a_old.png"]);
    }
}
