//! Рабочий цикл службы.
//!
//! Каждые poll_seconds:
//!   1) смотрим папку Google Диска: собираем пары «товар + этикетка»;
//!   2) готовые пары обрабатываем: штрихкод → сверка → 1С → фото карточки;
//!      брак — в папку _Ошибки со строкой в списке;
//!   3) понемногу перепроверяем цены/остатки уже сделанных карточек;
//!   4) если что-то изменилось — публикуем в общую папку киосков.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use image::{DynamicImage, ImageDecoder, ImageReader};

use crate::barcode;
use crate::config::Config;
use crate::names::{self, Kind, NameProblem, ParsedName};
use crate::onec::{OneC, OneCError};
use crate::publish::{self, HashCache};
use crate::store::{photo_name, Card, Store};

pub struct App {
    pub cfg: Config,
    onec: OneC,
    store: Store,
    cache: HashCache,
    /// Что мы видели в прошлый проход: путь → (размер, время изменения).
    seen: HashMap<PathBuf, (u64, u128)>,
    dirty: bool,
    last_signature: Option<String>,
    refresh_queue: Vec<String>,
    next_refresh: Instant,
    onec_down_logged: bool,
    /// Уже распознанные этикетки (чтобы не распознавать заново, пока 1С
    /// недоступна и пара ждёт).
    decoded_cache: HashMap<PathBuf, Vec<String>>,
}

pub fn now_human() -> String {
    chrono::Local::now().format("%d.%m.%Y %H:%M:%S").to_string()
}
fn now_version() -> String {
    chrono::Local::now().format("cards-%Y%m%d-%H%M%S").to_string()
}

impl App {
    pub fn new(cfg: Config) -> Result<App, String> {
        for d in [cfg.data(), cfg.data().join("photos"), cfg.data().join("archive")] {
            fs::create_dir_all(&d).map_err(|e| format!("не удалось создать {}: {e}", d.display()))?;
        }
        let onec = OneC::new(&cfg.api_url, &cfg.login, &cfg.password);
        let store = Store::load(&cfg.data().join("cards-db.json"));
        let cache = HashCache::load(&cfg.data().join("hash-cache.json"));
        Ok(App {
            cfg,
            onec,
            store,
            cache,
            seen: HashMap::new(),
            dirty: true, // при старте один раз сверяем общую папку с базой
            last_signature: None,
            refresh_queue: Vec::new(),
            next_refresh: Instant::now() + Duration::from_secs(60),
            onec_down_logged: false,
            decoded_cache: HashMap::new(),
        })
    }

    pub fn log(&self, msg: &str) {
        log_to(&self.cfg.data(), msg);
    }

    pub fn onec(&self) -> &OneC {
        &self.onec
    }

    /// Один проход цикла.
    pub fn tick(&mut self) {
        self.process_removals();
        if let Err(e) = self.scan_inbox() {
            self.log(&format!("папка снимков недоступна: {e}"));
        }
        self.refresh_some();
        self.publish_if_needed();
    }

    // ---------- 1. Папка снимков ----------

    fn scan_inbox(&mut self) -> Result<(), String> {
        let inbox = self.cfg.inbox();
        let rd = fs::read_dir(&inbox).map_err(|e| format!("{}: {e}", inbox.display()))?;

        let now = SystemTime::now();
        let mut groups: BTreeMap<String, Vec<(ParsedName, PathBuf)>> = BTreeMap::new();
        let mut new_seen = HashMap::new();
        let mut stable: HashMap<PathBuf, bool> = HashMap::new();
        let mut age_of: HashMap<PathBuf, Duration> = HashMap::new();

        for e in rd.flatten() {
            let path = e.path();
            let Ok(meta) = e.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            let mtime = meta.modified().unwrap_or(now);
            let mt_ms = mtime.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
            let sig = (meta.len(), mt_ms);
            let age = now.duration_since(mtime).unwrap_or(Duration::ZERO);
            // Файл «готов», если с прошлого прохода не менялся и ему хотя бы
            // несколько секунд — Google Диск мог ещё не докачать его.
            let is_stable = self.seen.get(&path) == Some(&sig) && age >= Duration::from_secs(3) && meta.len() > 0;
            new_seen.insert(path.clone(), sig);
            stable.insert(path.clone(), is_stable);
            age_of.insert(path.clone(), age);

            match names::parse(&name) {
                Ok(p) => groups.entry(p.group_key()).or_default().push((p, path)),
                Err(NameProblem::NotImage) => {}
                Err(problem) => {
                    if is_stable && age >= Duration::from_secs(120) {
                        let reason = match problem {
                            NameProblem::Heic => "снимок в формате HEIC — в быстрой команде должен быть шаг «Конвертировать изображение» в JPEG",
                            _ => "имя файла не по шаблону <штрихкод>__<дата-время>__photo|label.jpg",
                        };
                        self.reject(&[path], "", &[], reason);
                    }
                }
            }
        }
        self.seen = new_seen;
        let seen = &self.seen;
        self.decoded_cache.retain(|p, _| seen.contains_key(p));

        for (_key, files) in groups {
            if !files.iter().all(|(_, p)| stable.get(p).copied().unwrap_or(false)) {
                continue; // ещё докачивается
            }
            let photo = files.iter().filter(|(n, _)| n.kind == Kind::Photo).map(|(_, p)| p.clone()).last();
            let label = files.iter().filter(|(n, _)| n.kind == Kind::Label).map(|(_, p)| p.clone()).last();
            let code = files[0].0.code.clone();
            let all: Vec<PathBuf> = files.iter().map(|(_, p)| p.clone()).collect();
            match (photo, label) {
                (Some(photo), Some(label)) => self.process_pair(&code, &photo, &label, &all),
                _ => {
                    let oldest = all.iter().filter_map(|p| age_of.get(p)).max().copied().unwrap_or(Duration::ZERO);
                    if oldest >= Duration::from_secs(self.cfg.orphan_minutes * 60) {
                        self.reject(&all, &code, &[], "пришёл только один снимок из пары (нужны товар и этикетка)");
                    }
                }
            }
        }
        Ok(())
    }

    fn process_pair(&mut self, code: &str, photo: &Path, label: &Path, all: &[PathBuf]) {
        // а) Штрихкод на этикетке
        let decoded = match self.decoded_cache.get(label) {
            Some(d) => d.clone(),
            None => match open_image(label) {
                Ok(img) => {
                    let d = barcode::decode_all(&img);
                    self.decoded_cache.insert(label.to_path_buf(), d.clone());
                    d
                }
                Err(e) => {
                    self.reject(all, code, &[], &format!("не открывается снимок этикетки: {e}"));
                    return;
                }
            },
        };
        let matched = decoded.iter().any(|d| names::codes_match(d, code));
        if self.cfg.require_label_match && !matched {
            let reason = if decoded.is_empty() {
                "штрихкод на снимке этикетки не распознан (переснимите ближе и резче)".to_string()
            } else {
                "штрихкод на этикетке не совпадает с кодом в имени файла".to_string()
            };
            self.reject(all, code, &decoded, &reason);
            return;
        }

        // б) 1С
        let good = match self.onec.get_good(code) {
            Ok(g) => {
                self.onec_down_logged = false;
                g
            }
            Err(OneCError::Unavailable(e)) => {
                if !self.onec_down_logged {
                    self.log(&format!("1С недоступна, снимки подождут: {e}"));
                    self.onec_down_logged = true;
                }
                return; // файлы остаются, попробуем на следующем проходе
            }
            Err(e) => {
                self.reject(all, code, &decoded, &e.to_string());
                return;
            }
        };

        // в) Фото для карточки
        let photo_file = photo_name(code);
        let dst = self.cfg.data().join("photos").join(&photo_file);
        if let Err(e) = make_card_photo(photo, &dst, self.cfg.photo_max_side) {
            self.reject(all, code, &decoded, &format!("не открывается снимок товара: {e}"));
            return;
        }

        // г) База
        let now = now_human();
        let existed = self.store.cards.contains_key(code);
        let card = self.store.cards.entry(code.to_string()).or_insert_with(|| Card {
            code: code.to_string(),
            name: String::new(),
            characteristic: String::new(),
            price: 0.0,
            discounted_price: 0.0,
            discount_percent: 0.0,
            stock: None,
            group: None,
            photo: photo_file.clone(),
            found_in_1c: true,
            created: now.clone(),
            updated: now.clone(),
            checked: now.clone(),
        });
        card.apply_good(&good, &now);
        card.photo = photo_file;
        card.updated = now.clone();
        let line = format!(
            "{} карточка {}: {} — {:.2} ₽ по карте{}",
            if existed { "обновлена" } else { "новая" },
            code,
            good.name,
            good.discounted_price,
            match good.stock {
                Some(s) if s <= 0.0 => " (остаток 0 — на киосках не показывается)",
                _ => "",
            }
        );
        self.save_store();
        self.dirty = true;

        // д) Исходники — в архив (из Google Диска они уходят)
        let month = chrono::Local::now().format("%Y-%m").to_string();
        let arch = self.cfg.data().join("archive").join(month);
        let _ = fs::create_dir_all(&arch);
        for f in all {
            if let Some(n) = f.file_name() {
                let _ = move_file(f, &unique_path(&arch.join(n)));
            }
        }
        self.log(&line);
    }

    /// Брак: файлы — в _Ошибки, строка — в «список ошибок.csv».
    fn reject(&mut self, files: &[PathBuf], code: &str, decoded: &[String], reason: &str) {
        let dir = self.cfg.errors_dir();
        if let Err(e) = fs::create_dir_all(&dir) {
            self.log(&format!("не удалось создать {}: {e}", dir.display()));
            return;
        }
        let mut moved = Vec::new();
        for f in files {
            if let Some(n) = f.file_name() {
                let dst = unique_path(&dir.join(n));
                if move_file(f, &dst).is_ok() {
                    moved.push(dst.file_name().unwrap().to_string_lossy().to_string());
                }
                self.seen.remove(f);
            }
        }
        let list = dir.join("список ошибок.csv");
        let new_file = !list.exists();
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&list) {
            if new_file {
                // BOM, чтобы Excel сразу открыл кириллицу правильно
                let _ = f.write_all("\u{feff}Дата;Штрихкод в имени;Распознано на этикетке;Причина;Файлы\r\n".as_bytes());
            }
            let row = format!(
                "{};{};{};{};{}\r\n",
                now_human(),
                csv(code),
                csv(&decoded.join(" ")),
                csv(reason),
                csv(&moved.join(" + "))
            );
            let _ = f.write_all(row.as_bytes());
        }
        self.log(&format!("в _Ошибки: {} — {reason}", if code.is_empty() { moved.join(", ") } else { code.to_string() }));
    }

    /// Ручное удаление карточек: в рабочей папке службы создать файл
    /// «удалить.txt» и вписать в него штрихкоды (по одному в строке).
    /// Служба уберёт эти карточки с киосков и переименует файл в
    /// «удалить — выполнено <дата>.txt».
    fn process_removals(&mut self) {
        let path = self.cfg.data().join("удалить.txt");
        let Ok(text) = fs::read_to_string(&path) else { return };
        let mut removed = Vec::new();
        let mut unknown = Vec::new();
        for line in text.trim_start_matches('\u{feff}').lines() {
            let code = line.trim();
            if code.is_empty() {
                continue;
            }
            let key = self
                .store
                .cards
                .keys()
                .find(|k| names::codes_match(k, code))
                .cloned();
            match key {
                Some(k) => {
                    if let Some(c) = self.store.cards.remove(&k) {
                        let _ = fs::remove_file(self.cfg.data().join("photos").join(&c.photo));
                        removed.push(format!("{} {}", c.code, c.name));
                    }
                }
                None => unknown.push(code.to_string()),
            }
        }
        let done = self.cfg.data().join(format!("удалить — выполнено {}.txt", chrono::Local::now().format("%Y-%m-%d %H-%M-%S")));
        let _ = fs::rename(&path, &done);
        if !removed.is_empty() {
            self.save_store();
            self.dirty = true;
        }
        self.log(&format!(
            "удаление по списку: убрано {}{}",
            removed.len(),
            if unknown.is_empty() { String::new() } else { format!(", не найдены: {}", unknown.join(", ")) }
        ));
    }

    // ---------- 3. Перепроверка цен и остатков ----------

    fn refresh_some(&mut self) {
        if self.refresh_queue.is_empty() {
            if Instant::now() < self.next_refresh {
                return;
            }
            self.refresh_queue = self.store.cards.keys().cloned().collect();
            self.next_refresh = Instant::now() + Duration::from_secs(self.cfg.price_refresh_minutes.max(5) * 60);
            if self.refresh_queue.is_empty() {
                return;
            }
        }
        // Небольшими порциями, чтобы новые снимки не ждали.
        let mut changed = 0;
        for _ in 0..40 {
            let Some(code) = self.refresh_queue.pop() else { break };
            let now = now_human();
            match self.onec.get_good(&code) {
                Ok(g) => {
                    let mut msg = None;
                    if let Some(c) = self.store.cards.get_mut(&code) {
                        if c.apply_good(&g, &now) {
                            changed += 1;
                            msg = Some(format!("изменилось в 1С: {code} {} — {:.2} ₽{}", g.name, g.discounted_price, if c.visible() { "" } else { " (скрыта: остаток 0)" }));
                        }
                    }
                    if let Some(m) = msg {
                        self.log(&m);
                    }
                }
                Err(OneCError::NotFound) => {
                    let mut hidden = false;
                    if let Some(c) = self.store.cards.get_mut(&code) {
                        if c.found_in_1c {
                            c.found_in_1c = false;
                            c.updated = now.clone();
                            changed += 1;
                            hidden = true;
                        }
                        c.checked = now;
                    }
                    if hidden {
                        self.log(&format!("товар {code} больше не находится в 1С — карточка скрыта"));
                    }
                }
                Err(e) => {
                    // 1С недоступна — отложим весь круг на 5 минут.
                    self.refresh_queue.clear();
                    self.next_refresh = Instant::now() + Duration::from_secs(300);
                    if !self.onec_down_logged {
                        self.log(&format!("перепроверка цен отложена: {e}"));
                        self.onec_down_logged = true;
                    }
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if changed > 0 {
            self.save_store();
            self.dirty = true;
        }
    }

    // ---------- 4. Публикация ----------

    fn publish_if_needed(&mut self) {
        let out = self.cfg.output();
        let sig = match publish::output_signature(&out) {
            Ok(s) => s,
            Err(e) => {
                self.log(&format!("общая папка киосков недоступна: {e}"));
                return;
            }
        };
        if !self.dirty && self.last_signature.as_deref() == Some(sig.as_str()) {
            return;
        }
        let photos = self.cfg.data().join("photos");
        match publish::publish(&self.store, &photos, &out, &mut self.cache, &now_human(), &now_version()) {
            Ok(r) => {
                self.cache.save(&self.cfg.data().join("hash-cache.json"));
                self.dirty = false;
                self.last_signature = publish::output_signature(&out).ok();
                self.log(&format!(
                    "опубликовано для киосков: карточек {}, файлов {} (новых фото {}, убрано {}), версия {}",
                    r.cards, r.files, r.copied, r.removed, r.version
                ));
            }
            Err(e) => self.log(&format!("ОШИБКА публикации: {e}")),
        }
    }

    pub fn force_publish(&mut self) {
        self.dirty = true;
        self.publish_if_needed();
    }

    fn save_store(&self) {
        if let Err(e) = self.store.save(&self.cfg.data().join("cards-db.json")) {
            self.log(&format!("ОШИБКА записи базы карточек: {e}"));
        }
    }
}

// ---------- вспомогательное ----------

fn csv(s: &str) -> String {
    s.replace(';', ",").replace(['\r', '\n'], " ")
}

pub fn log_to(data_dir: &Path, msg: &str) {
    let line = format!("[{}] {msg}", now_human());
    println!("{line}");
    let path = data_dir.join("kiosk-cards.log");
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > 2_000_000 {
            let _ = fs::rename(&path, data_dir.join("kiosk-cards.old.log"));
        }
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

/// Открывает снимок с учётом поворота из EXIF (айфон часто пишет
/// «повернуть на 90°» тегом, а не поворачивает сами пиксели).
pub fn open_image(path: &Path) -> Result<DynamicImage, String> {
    let reader = ImageReader::open(path).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Ok(img)
}

fn make_card_photo(src: &Path, dst: &Path, max_side: u32) -> Result<(), String> {
    let img = open_image(src)?;
    let img = if img.width().max(img.height()) > max_side {
        img.resize(max_side, max_side, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    let tmp = dst.with_extension("jpg.tmp");
    {
        let f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        let mut w = std::io::BufWriter::new(f);
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, 82);
        rgb.write_with_encoder(enc).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, dst).map_err(|e| e.to_string())
}

/// Перенос файла; между дисками (Google Диск → C:) — копированием.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::remove_file(from)
}

fn unique_path(p: &Path) -> PathBuf {
    if !p.exists() {
        return p.to_path_buf();
    }
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = p.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    for i in 2..1000 {
        let c = p.with_file_name(format!("{stem} ({i}).{ext}"));
        if !c.exists() {
            return c;
        }
    }
    p.to_path_buf()
}
