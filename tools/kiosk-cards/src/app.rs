//! Рабочий цикл службы.
//!
//! Каждые poll_seconds:
//!   1) смотрим папку Google Диска: собираем снимки одного товара
//!      (ролик + фото обложки; штрихкод — в имени файлов);
//!   2) готовые обрабатываем: 1С → ролик и обложка карточки;
//!      брак — в папку _Ошибки со строкой в списке;
//!   3) понемногу перепроверяем цены/остатки уже сделанных карточек;
//!   4) если что-то изменилось — публикуем в общую папку киосков.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use image::{DynamicImage, ImageDecoder, ImageReader};

use crate::config::Config;
use crate::names::{self, Kind, NameProblem, ParsedName};
use crate::onec::{OneC, OneCError};
use crate::publish::{self, HashCache};
use crate::status::{self, Shared};
use crate::store::{photo_name, video_name, Card, Store};
use crate::video::{make_card_video, probe_size, VideoOptions};

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
    ffmpeg_missing_logged: bool,
    pub status: Shared,
    output_down_logged: bool,
    next_archive_cleanup: Instant,
    /// Ролики, которые не удалось пересобрать в новом качестве (нет
    /// исходника в архиве и т.п.) — больше не пытаемся до перезапуска.
    upgrade_skip: std::collections::HashSet<String>,
    /// Что последний раз записали в Google Диск для страницы телефона.
    last_phone_list: u64,
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
            ffmpeg_missing_logged: false,
            status: status::new_shared(),
            output_down_logged: false,
            next_archive_cleanup: Instant::now() + Duration::from_secs(120),
            upgrade_skip: Default::default(),
            last_phone_list: 0,
        })
    }

    pub fn log(&self, msg: &str) {
        log_to(&self.cfg.data(), msg);
    }

    pub fn onec(&self) -> &OneC {
        &self.onec
    }

    fn st(&self, f: impl FnOnce(&mut status::Status)) {
        if let Ok(mut g) = self.status.lock() {
            f(&mut g);
        }
    }

    fn event(&self, text: &str) {
        let t = format!("{} {}", chrono::Local::now().format("%H:%M"), text);
        self.st(|s| s.last_event = t);
    }

    /// Один проход цикла.
    pub fn tick(&mut self) {
        self.process_removals();
        match self.scan_inbox() {
            Ok(()) => self.st(|s| s.inbox_ok = true),
            Err(e) => {
                let was_ok = self.status.lock().map(|s| s.inbox_ok).unwrap_or(true);
                if was_ok {
                    self.log(&format!("папка снимков недоступна: {e}"));
                }
                self.st(|s| s.inbox_ok = false);
            }
        }
        self.process_phone_deletes();
        self.refresh_some();
        self.upgrade_video_one();
        self.publish_if_needed();
        self.write_phone_list();
        self.cleanup_archive();
        let total = self.store.cards.len();
        let visible = self.store.cards.values().filter(|c| c.visible()).count();
        let now = chrono::Local::now().format("%H:%M:%S").to_string();
        self.st(|s| {
            s.cards_total = total;
            s.cards_visible = visible;
            s.last_tick = now;
        });
    }

    /// Исходники с телефона после обработки лежат в archive\<год-месяц>.
    /// Старше archive_days дней — удаляем, чтобы не забивать диск PC-38.
    fn cleanup_archive(&mut self) {
        if Instant::now() < self.next_archive_cleanup || self.cfg.archive_days == 0 {
            return;
        }
        self.next_archive_cleanup = Instant::now() + Duration::from_secs(6 * 3600);
        let limit = Duration::from_secs(self.cfg.archive_days * 86400);
        let now = SystemTime::now();
        let mut removed = 0;
        let Ok(months) = fs::read_dir(self.cfg.data().join("archive")) else { return };
        for m in months.flatten() {
            let Ok(files) = fs::read_dir(m.path()) else { continue };
            let mut left = 0;
            for f in files.flatten() {
                let old = f
                    .metadata()
                    .and_then(|md| md.modified())
                    .map(|t| now.duration_since(t).unwrap_or(Duration::ZERO) > limit)
                    .unwrap_or(false);
                if old && fs::remove_file(f.path()).is_ok() {
                    removed += 1;
                } else {
                    left += 1;
                }
            }
            if left == 0 {
                let _ = fs::remove_dir(m.path());
            }
        }
        if removed > 0 {
            self.log(&format!("архив исходников: удалено старше {} дней — {removed} файлов", self.cfg.archive_days));
        }
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
                            _ => "имя файла не по шаблону <штрихкод>__<дата-время>__video|cover",
                        };
                        self.reject(&[path], "", reason);
                    }
                }
            }
        }
        self.seen = new_seen;

        for (_key, files) in groups {
            if !files.iter().all(|(_, p)| stable.get(p).copied().unwrap_or(false)) {
                continue; // ещё докачивается
            }
            // для товара ролик важнее фото, если прислали и то и другое
            let product = files
                .iter()
                .filter(|(n, _)| n.kind == Kind::Photo && n.video)
                .chain(files.iter().filter(|(n, _)| n.kind == Kind::Photo && !n.video))
                .map(|(_, p)| p.clone())
                .next();
            let cover = files.iter().filter(|(n, _)| n.kind == Kind::Cover).map(|(_, p)| p.clone()).last();
            let code = files[0].0.code.clone();
            let all: Vec<PathBuf> = files.iter().map(|(_, p)| p.clone()).collect();
            // Телефон присылает ролик и обложку почти одновременно. Если
            // пришло что-то одно — ждём вторую часть пару минут, потом
            // делаем карточку из того, что есть.
            let oldest = all.iter().filter_map(|p| age_of.get(p)).max().copied().unwrap_or(Duration::ZERO);
            let complete = product.is_some() && cover.is_some();
            if complete || oldest >= Duration::from_secs(120) {
                self.process_item(&code, product.as_deref(), cover.as_deref(), &all);
            }
        }
        Ok(())
    }

    fn process_item(&mut self, code: &str, product: Option<&Path>, cover: Option<&Path>, all: &[PathBuf]) {
        // а) 1С: название и цена по штрихкоду из имени файла
        let good = match self.onec.get_good(code) {
            Ok(g) => {
                self.onec_down_logged = false;
                self.st(|s| s.onec_ok = Some(true));
                g
            }
            Err(OneCError::Unavailable(e)) => {
                if !self.onec_down_logged {
                    self.log(&format!("1С недоступна, снимки подождут: {e}"));
                    self.onec_down_logged = true;
                }
                self.st(|s| s.onec_ok = Some(false));
                return; // файлы остаются, попробуем на следующем проходе
            }
            Err(e) => {
                self.st(|s| s.onec_ok = Some(true));
                self.reject(all, code, &e.to_string());
                return;
            }
        };

        // б) Ролик и обложка карточки
        let photo_file = photo_name(code);
        let photos_dir = self.cfg.data().join("photos");
        let dst = photos_dir.join(&photo_file);
        let is_video = product
            .and_then(|p| p.file_name())
            .and_then(|n| names::parse(&n.to_string_lossy()).ok())
            .map(|p| p.video)
            .unwrap_or(false);
        let mut src_size: Option<(u32, u32)> = None;
        let video_file = match (product, is_video) {
            (Some(product), true) => {
                let vname = video_name(code);
                let opts = self.video_opts();
                src_size = probe_size(&self.cfg.ffmpeg(), product);
                if let Err(e) = make_card_video(&self.cfg.ffmpeg(), product, &photos_dir.join(&vname), &dst, &opts) {
                    if e.starts_with("не запускается ffmpeg") {
                        // не брак снимка, а нет ffmpeg — ждём, файлы не трогаем
                        if !self.ffmpeg_missing_logged {
                            self.log(&format!("{e} — положите ffmpeg.exe рядом со службой; ролики подождут"));
                            self.ffmpeg_missing_logged = true;
                        }
                        self.st(|s| s.ffmpeg_ok = false);
                        return;
                    }
                    self.reject(all, code, &format!("не получается обработать ролик товара: {e}"));
                    return;
                }
                self.st(|s| s.ffmpeg_ok = true);
                Some(vname)
            }
            _ => None,
        };
        // Обложка: фото обложки, если прислали; иначе снимок товара; у
        // ролика без обложки остаётся кадр из середины ролика.
        let still = cover.or(if is_video { None } else { product });
        if let Some(still) = still {
            if let Err(e) = make_card_photo(still, &dst, self.cfg.photo_max_side) {
                if video_file.is_none() {
                    self.reject(all, code, &format!("не открывается фото обложки: {e}"));
                    return;
                }
                self.log(&format!("{code}: фото обложки не открылось ({e}), обложкой будет кадр из ролика"));
            }
        }
        if video_file.is_none() {
            // товар пересняли без ролика — старый ролик больше не нужен
            let _ = fs::remove_file(photos_dir.join(video_name(code)));
        }

        // в) База
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
            video: None,
            found_in_1c: true,
            created: now.clone(),
            updated: now.clone(),
            checked: now.clone(),
            video_side: None,
        });
        card.apply_good(&good, &now);
        card.photo = photo_file;
        card.video = video_file.clone();
        card.video_side = video_file.as_ref().map(|_| self.cfg.video_max_side);
        card.updated = now.clone();
        let line = format!(
            "{} карточка {}{}{}: {} — {:.2} ₽ по карте{}",
            if existed { "обновлена" } else { "новая" },
            code,
            match (&video_file, src_size) {
                (Some(_), Some((w, h))) => format!(" (ролик, с телефона пришёл {w}×{h}{})", if w.max(h) < 1280 { " — НИЗКОЕ качество" } else { "" }),
                (Some(_), None) => " (ролик)".to_string(),
                _ => String::new(),
            },
            if cover.is_some() { " (обложка)" } else { "" },
            good.name,
            good.discounted_price,
            match good.stock {
                Some(s) if s <= 0.0 => " (остаток 0 — на киосках не показывается)",
                _ => "",
            }
        );
        self.save_store();
        self.dirty = true;

        // г) Исходники — в архив (из Google Диска они уходят)
        let month = chrono::Local::now().format("%Y-%m").to_string();
        let arch = self.cfg.data().join("archive").join(month);
        let _ = fs::create_dir_all(&arch);
        for f in all {
            if let Some(n) = f.file_name() {
                let _ = move_file(f, &unique_path(&arch.join(n)));
            }
        }
        self.log(&line);
        let short = format!("{} карточка {code}: {}", if existed { "обновлена" } else { "новая" }, good.name);
        self.event(&short);
        self.st(|s| s.processed += 1);
    }

    fn video_opts(&self) -> VideoOptions {
        VideoOptions {
            max_side: self.cfg.video_max_side,
            seconds: self.cfg.video_seconds,
            boomerang: self.cfg.video_boomerang,
            crf: self.cfg.video_crf,
        }
    }

    /// Ролики, сделанные со старыми настройками качества, понемногу (по
    /// одному за проход) пересобираем из исходников в архиве. Обложку не
    /// трогаем.
    fn upgrade_video_one(&mut self) {
        let want = self.cfg.video_max_side;
        let Some(code) = self
            .store
            .cards
            .values()
            .find(|c| c.video.is_some() && c.video_side != Some(want) && !self.upgrade_skip.contains(&c.code))
            .map(|c| c.code.clone())
        else {
            return;
        };
        let Some(src) = self.find_archived_video(&code) else {
            self.upgrade_skip.insert(code.clone());
            self.log(&format!("{code}: исходника ролика в архиве нет — останется в прежнем качестве (переснимите, если нужно)"));
            return;
        };
        let photos = self.cfg.data().join("photos");
        let vname = video_name(&code);
        let tmp_poster = photos.join(format!("{vname}.poster.jpg"));
        let size = probe_size(&self.cfg.ffmpeg(), &src);
        match make_card_video(&self.cfg.ffmpeg(), &src, &photos.join(&vname), &tmp_poster, &self.video_opts()) {
            Ok(()) => {
                let _ = fs::remove_file(&tmp_poster);
                if let Some(c) = self.store.cards.get_mut(&code) {
                    c.video = Some(vname);
                    c.video_side = Some(want);
                }
                self.save_store();
                self.dirty = true;
                self.log(&format!(
                    "{code}: ролик пересобран в новом качестве{}",
                    size.map(|(w, h)| format!(" (исходник {w}×{h})")).unwrap_or_default()
                ));
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp_poster);
                self.upgrade_skip.insert(code.clone());
                if !e.starts_with("не запускается ffmpeg") {
                    self.log(&format!("{code}: ролик не пересобран: {e}"));
                }
            }
        }
    }

    /// Самый свежий исходный ролик товара в archive\<год-месяц>.
    fn find_archived_video(&self, code: &str) -> Option<PathBuf> {
        let mut best: Option<(String, PathBuf)> = None;
        let months = fs::read_dir(self.cfg.data().join("archive")).ok()?;
        for m in months.flatten() {
            let Ok(files) = fs::read_dir(m.path()) else { continue };
            for f in files.flatten() {
                let name = f.file_name().to_string_lossy().to_string();
                // unique_path мог дописать « (2)» — сравниваем по началу имени
                let Ok(p) = names::parse(&name) else { continue };
                if p.video && p.kind == Kind::Photo && names::codes_match(&p.code, code) {
                    if best.as_ref().map_or(true, |(st, _)| p.stamp > *st) {
                        best = Some((p.stamp.clone(), f.path()));
                    }
                }
            }
        }
        best.map(|(_, p)| p)
    }

    // ---------- Страница телефона: список карточек и удаление ----------

    fn phone_dir(&self) -> PathBuf {
        self.cfg.inbox().join("_Карточки")
    }

    /// Пишет в Google Диск (папка «Карточки Товаров\_Карточки») список
    /// карточек с маленькими картинками — его показывает страница
    /// телефона в разделе «Карточки». Пишем, только если список изменился.
    fn write_phone_list(&mut self) {
        let thumbs_dir = self.cfg.data().join("thumbs");
        let _ = fs::create_dir_all(&thumbs_dir);
        let photos = self.cfg.data().join("photos");
        let mut list: Vec<&Card> = self.store.cards.values().collect();
        list.sort_by(|a, b| b.updated.cmp(&a.updated));
        let mut items = Vec::new();
        for c in list {
            let thumb = thumb_data_url(&photos.join(&c.photo), &thumbs_dir.join(format!("{}.jpg", c.code)));
            items.push(serde_json::json!({
                "code": c.code,
                "name": c.name,
                "characteristic": c.characteristic,
                "price": c.price,
                "discounted_price": c.discounted_price,
                "visible": c.visible(),
                "video": c.video.is_some(),
                "updated": c.updated,
                "thumb": thumb,
            }));
        }
        let body = serde_json::json!({ "cards": items });
        let text = body.to_string();
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut h);
            h.finish()
        };
        if hash == self.last_phone_list {
            return;
        }
        let dir = self.phone_dir();
        if fs::create_dir_all(&dir).is_err() {
            return; // Google Диска нет — об этом уже говорит значок
        }
        let full = serde_json::json!({ "generated": now_human(), "cards": body["cards"] }).to_string();
        let tmp = dir.join("cards.json.tmp");
        let dst = dir.join("cards.json");
        if fs::write(&tmp, full.as_bytes()).is_ok() {
            // rename поверх существующего: Google Диск обновит тот же файл
            if fs::rename(&tmp, &dst).is_ok() {
                self.last_phone_list = hash;
            }
        }
    }

    /// Удаление с телефона: страница кладёт в «Карточки Товаров\_Удалить»
    /// файлы <штрихкод>.txt. Убираем эти карточки с киосков, файлы-просьбы
    /// удаляем.
    fn process_phone_deletes(&mut self) {
        let dir = self.cfg.inbox().join("_Удалить");
        let Ok(rd) = fs::read_dir(&dir) else { return };
        let mut codes = Vec::new();
        let mut files = Vec::new();
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().map_or(true, |x| !x.eq_ignore_ascii_case("txt")) {
                continue;
            }
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let text = fs::read_to_string(&path).unwrap_or_default();
            let mut any = false;
            for line in text.trim_start_matches('\u{feff}').lines().map(str::trim).filter(|l| !l.is_empty()) {
                codes.push(line.to_string());
                any = true;
            }
            if !any && !stem.is_empty() {
                codes.push(stem);
            }
            files.push(path);
        }
        if files.is_empty() {
            return;
        }
        let (removed, unknown) = self.remove_codes(&codes);
        for f in &files {
            let _ = fs::remove_file(f);
        }
        if !removed.is_empty() {
            self.save_store();
            self.dirty = true;
            self.event(&format!("удалено с телефона: {}", removed.len()));
        }
        self.log(&format!(
            "удаление с телефона: убрано {}{}{}",
            removed.len(),
            if removed.is_empty() { String::new() } else { format!(" ({})", removed.join("; ")) },
            if unknown.is_empty() { String::new() } else { format!(", не найдены: {}", unknown.join(", ")) }
        ));
    }

    /// Убирает карточки по штрихкодам. Возвращает (убранные, не найденные).
    fn remove_codes(&mut self, codes: &[String]) -> (Vec<String>, Vec<String>) {
        let mut removed = Vec::new();
        let mut unknown = Vec::new();
        for code in codes {
            let key = self.store.cards.keys().find(|k| names::codes_match(k, code)).cloned();
            match key {
                Some(k) => {
                    if let Some(c) = self.store.cards.remove(&k) {
                        let _ = fs::remove_file(self.cfg.data().join("photos").join(&c.photo));
                        if let Some(v) = &c.video {
                            let _ = fs::remove_file(self.cfg.data().join("photos").join(v));
                        }
                        let _ = fs::remove_file(self.cfg.data().join("thumbs").join(format!("{}.jpg", c.code)));
                        removed.push(format!("{} {}", c.code, c.name));
                    }
                }
                None => unknown.push(code.clone()),
            }
        }
        (removed, unknown)
    }

    /// Брак: файлы — в _Ошибки, строка — в «список ошибок.csv».
    fn reject(&mut self, files: &[PathBuf], code: &str, reason: &str) {
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
                let _ = f.write_all("\u{feff}Дата;Штрихкод;Причина;Файлы\r\n".as_bytes());
            }
            let row = format!(
                "{};{};{};{}\r\n",
                now_human(),
                csv(code),
                csv(reason),
                csv(&moved.join(" + "))
            );
            let _ = f.write_all(row.as_bytes());
        }
        let who = if code.is_empty() { moved.join(", ") } else { code.to_string() };
        self.log(&format!("в _Ошибки: {who} — {reason}"));
        self.event(&format!("в _Ошибки: {who} — {reason}"));
        self.st(|s| s.rejected += 1);
    }

    /// Ручное удаление карточек: в рабочей папке службы создать файл
    /// «удалить.txt» и вписать в него штрихкоды (по одному в строке).
    /// Служба уберёт эти карточки с киосков и переименует файл в
    /// «удалить — выполнено <дата>.txt».
    fn process_removals(&mut self) {
        let path = self.cfg.data().join("удалить.txt");
        let Ok(text) = fs::read_to_string(&path) else { return };
        let codes: Vec<String> = text
            .trim_start_matches('\u{feff}')
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let (removed, unknown) = self.remove_codes(&codes);
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
                    self.st(|s| s.onec_ok = Some(true));
                    self.onec_down_logged = false;
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
                    self.st(|s| s.onec_ok = Some(false));
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
                if !self.output_down_logged {
                    self.log(&format!("общая папка киосков недоступна: {e}"));
                    self.output_down_logged = true;
                }
                self.st(|s| s.output_ok = false);
                return;
            }
        };
        if self.output_down_logged {
            self.log("общая папка киосков снова доступна");
            self.output_down_logged = false;
        }
        self.st(|s| s.output_ok = true);
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
            Err(e) => {
                self.log(&format!("ОШИБКА публикации: {e}"));
                self.event(&format!("ошибка публикации: {e}"));
            }
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

/// Маленькая картинка для списка на телефоне (data:URL), с кешем в
/// data\thumbs — чтобы не пережимать каждый раз.
fn thumb_data_url(photo: &Path, cache: &Path) -> String {
    use base64::Engine;
    let fresh = match (fs::metadata(photo).and_then(|m| m.modified()), fs::metadata(cache).and_then(|m| m.modified())) {
        (Ok(p), Ok(c)) => c >= p,
        _ => false,
    };
    if !fresh {
        let Ok(img) = open_image(photo) else { return String::new() };
        let img = img.thumbnail(220, 220).to_rgb8();
        let tmp = cache.with_extension("tmp");
        let ok = (|| -> Result<(), String> {
            let f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
            let mut w = std::io::BufWriter::new(f);
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, 70);
            img.write_with_encoder(enc).map_err(|e| e.to_string())
        })();
        if ok.is_err() || fs::rename(&tmp, cache).is_err() {
            return String::new();
        }
    }
    match fs::read(cache) {
        Ok(b) => format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(b)),
        Err(_) => String::new(),
    }
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
