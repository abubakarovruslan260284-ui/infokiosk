//! kiosk_cards — служба карточек товаров для инфокиоска.
//!
//! Айфон (быстрая команда) → папка Google Диска → эта служба →
//! 1С (название, цена по карте) → общая папка киосков
//! (\\wdmycloud\Public\InfoContent): фото card_<код>.jpg + cards.json +
//! manifest.json. Киоски забирают это как обычный контент.
//!
//! Запуск:
//!   kiosk_cards                 — работать постоянно (обычный режим)
//!   kiosk_cards --once          — один проход и выход
//!   kiosk_cards --test-1c КОД   — проверить связь с 1С по штрихкоду
//!   kiosk_cards --decode ФАЙЛ   — распознать штрихкод на снимке
//!   kiosk_cards --publish       — переопубликовать общую папку
//!   kiosk_cards --autostart     — запускать службу при входе в Windows

mod app;
mod barcode;
mod config;
mod names;
mod onec;
mod publish;
mod store;
mod video;

use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.first().map(|s| s.as_str()) == Some("--decode") {
        let Some(path) = args.get(1) else {
            eprintln!("Использование: kiosk_cards --decode <снимок>");
            std::process::exit(2);
        };
        match app::open_image(std::path::Path::new(path)) {
            Ok(img) => {
                let codes = barcode::decode_all(&img);
                if codes.is_empty() {
                    println!("Штрихкод не распознан.");
                } else {
                    for c in codes {
                        println!("Распознано: {c}");
                    }
                }
            }
            Err(e) => eprintln!("Не удалось открыть снимок: {e}"),
        }
        return;
    }

    if args.first().map(|s| s.as_str()) == Some("--autostart") {
        match install_autostart() {
            Ok(p) => println!("Готово: служба будет сама запускаться при входе в Windows.\nФайл автозапуска: {}", p.display()),
            Err(e) => eprintln!("Не удалось настроить автозапуск: {e}"),
        }
        pause();
        return;
    }

    let cfg_path = config::Config::path_next_to_exe();
    let cfg = match config::Config::load_or_create(&cfg_path) {
        Ok(Some(c)) => c,
        Ok(None) => {
            println!("Создан файл настроек: {}", cfg_path.display());
            println!("Откройте его Блокнотом, впишите логин и пароль 1С, проверьте пути и запустите службу снова.");
            pause();
            return;
        }
        Err(e) => {
            eprintln!("{e}");
            pause();
            std::process::exit(1);
        }
    };
    if cfg.login.trim().is_empty() {
        eprintln!("В {} не заполнен логин 1С (\"login\").", cfg_path.display());
        pause();
        std::process::exit(1);
    }

    let mut app = match app::App::new(cfg) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            pause();
            std::process::exit(1);
        }
    };

    match args.first().map(|s| s.as_str()) {
        Some("--test-1c") => {
            let code = args.get(1).cloned().unwrap_or_else(|| "4680269373655".to_string());
            match app.onec().get_good(&code) {
                Ok(g) => {
                    println!("1С ответила по {code}:");
                    println!("  название:       {}", g.name);
                    println!("  характеристика: {}", g.characteristic);
                    println!("  цена:           {:.2}", g.price);
                    println!("  цена по карте:  {:.2}", g.discounted_price);
                    println!("  остаток:        {}", g.stock.map(|s| s.to_string()).unwrap_or_else(|| "1С пока не отдаёт".into()));
                    println!("  группа:         {}", g.group.unwrap_or_else(|| "1С пока не отдаёт".into()));
                }
                Err(e) => println!("Ошибка: {e}"),
            }
        }
        Some("--publish") => app.force_publish(),
        Some("--once") => {
            // Два прохода: файл считается докачанным, только если между
            // проходами не изменился.
            app.tick();
            std::thread::sleep(Duration::from_secs(4));
            app.tick();
        }
        _ => {
            app.log(&format!(
                "=== служба карточек запущена: снимки из «{}», карточки в «{}» ===",
                app.cfg.inbox_dir, app.cfg.output_dir
            ));
            match video::ffmpeg_version(&app.cfg.ffmpeg()) {
                Some(v) => app.log(&format!("ролики: {v}")),
                None => app.log("ВНИМАНИЕ: ffmpeg не найден — ролики обрабатываться не будут (фото — будут). Положите ffmpeg.exe рядом со службой."),
            }
            let poll = Duration::from_secs(app.cfg.poll_seconds.max(5));
            loop {
                app.tick();
                std::thread::sleep(poll);
            }
        }
    }
}

/// Чтобы окно не закрывалось мгновенно при запуске двойным щелчком.
fn pause() {
    println!("\nНажмите Enter, чтобы закрыть окно...");
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
}

/// Кладёт в папку «Автозагрузка» текущего пользователя маленький .cmd,
/// который запускает службу свёрнутым окном. Именно при входе
/// пользователя, а не как системная служба Windows: «Google Диск для
/// компьютера» (и его диск G:) работает только в сеансе пользователя.
fn install_autostart() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let appdata = std::env::var("APPDATA").map_err(|_| "не найдена папка APPDATA".to_string())?;
    let startup = std::path::Path::new(&appdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup");
    std::fs::create_dir_all(&startup).map_err(|e| e.to_string())?;
    let cmd = startup.join("kiosk_cards.cmd");
    let dir = exe.parent().map(|d| d.display().to_string()).unwrap_or_default();
    // chcp 65001 — чтобы кириллица в путях читалась правильно
    let text = format!(
        "@echo off\r\nchcp 65001 >nul\r\nrem Служба карточек инфокиоска — запуск при входе в Windows\r\ncd /d \"{dir}\"\r\nstart \"Служба карточек\" /min \"{}\"\r\n",
        exe.display()
    );
    std::fs::write(&cmd, text).map_err(|e| e.to_string())?;
    Ok(cmd)
}
