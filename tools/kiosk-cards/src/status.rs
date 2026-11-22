//! Состояние службы для значка возле часов (и для журнала).
//! Рабочий поток пишет сюда, значок раз в секунду читает.


#![cfg_attr(not(windows), allow(dead_code))]
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct Status {
    /// Когда служба запущена (чч:мм дд.мм).
    pub started: String,
    /// Когда закончился последний проход.
    pub last_tick: String,
    /// Папка Google Диска со снимками читается.
    pub inbox_ok: bool,
    /// Общая папка киосков (WD MyCloud) доступна.
    pub output_ok: bool,
    /// Ответ 1С: None — ещё не спрашивали.
    pub onec_ok: Option<bool>,
    pub ffmpeg_ok: bool,
    /// Карточек показывается на киосках / всего в базе.
    pub cards_visible: usize,
    pub cards_total: usize,
    /// С момента запуска.
    pub processed: u32,
    pub rejected: u32,
    /// Последнее событие: «13:23 новая карточка …».
    pub last_event: String,
    /// Просьба из меню: «проверить папку сейчас».
    pub wake: bool,
}

pub type Shared = Arc<Mutex<Status>>;

pub fn new_shared() -> Shared {
    Arc::new(Mutex::new(Status {
        started: chrono::Local::now().format("%H:%M %d.%m").to_string(),
        inbox_ok: true,
        output_ok: true,
        ffmpeg_ok: true,
        ..Default::default()
    }))
}

/// Общая оценка: 0 — всё хорошо, 1 — есть временная беда (1С, ffmpeg),
/// 2 — служба не может работать (нет Google Диска или папки киосков).
pub fn level(s: &Status) -> u8 {
    if !s.inbox_ok || !s.output_ok {
        2
    } else if s.onec_ok == Some(false) || !s.ffmpeg_ok {
        1
    } else {
        0
    }
}

/// Главная строка состояния.
pub fn headline(s: &Status) -> String {
    if !s.inbox_ok {
        "Нет доступа к папке Google Диска".into()
    } else if !s.output_ok {
        "Нет доступа к общей папке киосков".into()
    } else if s.onec_ok == Some(false) {
        "Работает, но 1С не отвечает".into()
    } else if !s.ffmpeg_ok {
        "Работает, но нет ffmpeg (ролики ждут)".into()
    } else {
        "Работает".into()
    }
}
