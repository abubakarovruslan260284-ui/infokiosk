//! Значок службы в области уведомлений Windows (возле часов).
//!
//! Цвет значка: зелёный — всё работает; жёлтый — 1С не отвечает или нет
//! ffmpeg (снимки ждут); красный — нет Google Диска или общей папки
//! киосков. Подсказка при наведении и меню по правой кнопке показывают
//! подробности; двойной щелчок открывает журнал.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::Console::{GetConsoleProcessList, GetConsoleWindow};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MessageBoxW, PeekMessageW, ShowWindow, TranslateMessage, MB_ICONINFORMATION, MB_OK, MSG,
    PM_REMOVE, SW_HIDE,
};

use crate::status::{self, Shared};

pub struct Paths {
    pub log: PathBuf,
    pub errors: PathBuf,
    pub output: PathBuf,
    pub data: PathBuf,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Вторая копия службы не нужна: она бы делила с первой одни и те же
/// файлы. Возвращает «замок», который держим до выхода.
pub fn single_instance() -> Option<HANDLE> {
    let name = wide("Local\\InfokioskKioskCardsService");
    unsafe {
        let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if h.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
            None
        } else {
            Some(h)
        }
    }
}

pub fn message(text: &str) {
    let t = wide(text);
    let c = wide("Служба карточек");
    unsafe {
        MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), MB_OK | MB_ICONINFORMATION);
    }
}

/// Прячет чёрное окно, если оно — собственное окно службы (запуск двойным
/// щелчком или из автозапуска). Если службу запустили из уже открытой
/// командной строки, её окно не трогаем.
pub fn hide_own_console() {
    unsafe {
        let mut list = [0u32; 4];
        let n = GetConsoleProcessList(list.as_mut_ptr(), list.len() as u32);
        if n == 1 {
            let w = GetConsoleWindow();
            if !w.is_null() {
                ShowWindow(w, SW_HIDE);
            }
        }
    }
}

/// Значок 32×32: цветной круг с белой «карточкой».
fn make_icon(level: u8) -> Icon {
    let (r, g, b) = match level {
        0 => (0x18, 0xa8, 0x73),
        1 => (0xf0, 0xa0, 0x20),
        _ => (0xd9, 0x30, 0x25),
    };
    let n = 32usize;
    let mut px = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let i = (y * n + x) * 4;
            let dx = x as f32 + 0.5 - 16.0;
            let dy = y as f32 + 0.5 - 16.0;
            let d = (dx * dx + dy * dy).sqrt();
            if d > 15.5 {
                continue;
            }
            let a = if d > 14.5 { ((15.5 - d) * 255.0) as u8 } else { 255 };
            let card = (9..23).contains(&x) && (8..24).contains(&y);
            let bar = card && (11..21).contains(&x) && (18..21).contains(&y);
            let (pr, pg, pb) = if bar { (r, g, b) } else if card { (255, 255, 255) } else { (r, g, b) };
            px[i] = pr;
            px[i + 1] = pg;
            px[i + 2] = pb;
            px[i + 3] = a;
        }
    }
    Icon::from_rgba(px, n as u32, n as u32).expect("значок")
}

fn open(path: &PathBuf) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

fn open_log(path: &PathBuf) {
    let _ = std::process::Command::new("notepad.exe").arg(path).spawn();
}

fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Значок и его меню. Работает в текущем потоке до «Остановить службу».
pub fn run(status: Shared, paths: Paths) {
    let m_head = MenuItem::new("Служба карточек: запускается…", false, None);
    let m_cards = MenuItem::new("Карточек на киосках: —", false, None);
    let m_onec = MenuItem::new("1С: ещё не проверялась", false, None);
    let m_count = MenuItem::new("С запуска: обработано 0, в ошибках 0", false, None);
    let m_last = MenuItem::new("Последнее: —", false, None);
    let m_now = MenuItem::new("Проверить папку сейчас", true, None);
    let m_log = MenuItem::new("Открыть журнал", true, None);
    let m_err = MenuItem::new("Открыть папку ошибок (_Ошибки)", true, None);
    let m_out = MenuItem::new("Открыть папку киосков (InfoContent)", true, None);
    let m_data = MenuItem::new("Открыть папку службы", true, None);
    let m_quit = MenuItem::new("Остановить службу", true, None);

    let menu = Menu::new();
    let _ = menu.append_items(&[
        &m_head,
        &m_cards,
        &m_onec,
        &m_count,
        &m_last,
        &PredefinedMenuItem::separator(),
        &m_now,
        &m_log,
        &m_err,
        &m_out,
        &m_data,
        &PredefinedMenuItem::separator(),
        &m_quit,
    ]);

    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Служба карточек инфокиоска")
        .with_icon(make_icon(0))
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            crate::app::log_to(&paths.data, &format!("значок возле часов не создан: {e}"));
            // без значка служба всё равно работает
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
    };

    let mut last_level = 0u8;
    let mut next_update = Instant::now();
    loop {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == *m_now.id() {
                if let Ok(mut s) = status.lock() {
                    s.wake = true;
                }
            } else if ev.id == *m_log.id() {
                open_log(&paths.log);
            } else if ev.id == *m_err.id() {
                let _ = std::fs::create_dir_all(&paths.errors);
                open(&paths.errors);
            } else if ev.id == *m_out.id() {
                open(&paths.output);
            } else if ev.id == *m_data.id() {
                open(&paths.data);
            } else if ev.id == *m_quit.id() {
                crate::app::log_to(&paths.data, "=== служба карточек остановлена из меню значка ===");
                drop(tray);
                std::process::exit(0);
            }
        }
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = ev {
                open_log(&paths.log);
            }
        }

        if Instant::now() >= next_update {
            next_update = Instant::now() + Duration::from_secs(1);
            let s = status.lock().map(|g| g.clone()).unwrap_or_default();
            let level = status::level(&s);
            if level != last_level {
                let _ = tray.set_icon(Some(make_icon(level)));
                last_level = level;
            }
            let head = format!("{} (с {})", status::headline(&s), s.started);
            m_head.set_text(&head);
            m_cards.set_text(format!("Карточек на киосках: {} (всего {})", s.cards_visible, s.cards_total));
            m_onec.set_text(match s.onec_ok {
                None => "1С: ещё не спрашивали (нет новых снимков)".to_string(),
                Some(true) => "1С: отвечает".to_string(),
                Some(false) => "1С: НЕ отвечает — снимки ждут".to_string(),
            });
            m_count.set_text(format!(
                "С запуска: обработано {}, в ошибках {}{}",
                s.processed,
                s.rejected,
                if s.last_tick.is_empty() { String::new() } else { format!(" · проверка {}", s.last_tick) }
            ));
            m_last.set_text(if s.last_event.is_empty() {
                "Последнее: —".to_string()
            } else {
                format!("Последнее: {}", cut(&s.last_event, 90))
            });
            let tip = format!(
                "Служба карточек: {}\nНа киосках: {} · 1С: {}",
                status::headline(&s),
                s.cards_visible,
                match s.onec_ok {
                    None => "—",
                    Some(true) => "ок",
                    Some(false) => "нет связи",
                }
            );
            let _ = tray.set_tooltip(Some(cut(&tip, 120)));
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}
