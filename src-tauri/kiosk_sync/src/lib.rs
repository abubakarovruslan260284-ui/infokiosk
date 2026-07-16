//! kiosk_sync — движок фоновой синхронизации контента инфокиоска
//! с общей сетевой папкой (или локальной папкой «Издателя»).
//!
//! Идея в одном абзаце: киоск никогда не показывает контент "с сети".
//! Он периодически (дёшево) проверяет один маленький файл — манифест —
//! и, только если его версия изменилась, докачивает во временную папку
//! ровно то, что реально поменялось (по SHA-256), проверяет целостность
//! и одним атомарным переименованием подменяет рабочий кэш. Показ
//! контента на экране всегда идёт из локального кэша на диске.

pub mod diff;
pub mod error;
pub mod manifest;
pub mod sync;

pub use diff::{plan_sync, Plan};
pub use error::SyncError;
pub use manifest::{FileEntry, Manifest};
pub use sync::{active_cache_dir, read_local_manifest, read_remote_manifest, sync_once, version_changed, SyncReport};
