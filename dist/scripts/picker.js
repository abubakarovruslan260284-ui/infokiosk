// Подбор по цене — только на сенсорном киоске в режиме "picker"
// (F3 → Режим киоска). Остальные киоски этого не показывают.
//
// Сценарий (его же снимает СММщик для сторис):
//   1) посетитель касается кнопки «Подобрать товар по цене»;
//   2) выбирает сумму;
//   3) киоск «собирает подборку у него на глазах»: мелькают товары,
//      по шагам идёт «подключение, отбор, цены, монтаж», бежит процент;
//      настоящий отбор при этом уже сделан — карточки готовы заранее
//      службой на PC-38 и лежат в локальном кэше киоска;
//   4) готовый коллаж из отобранных товаров;
//   5) ролики товаров по одному, крупно;
//   6) возврат к витрине.
// Цены у товаров в подборке не показываются — только диапазон в заголовке.

(function () {
  const RANGES = [
    { label: "до 100 ₽", min: 0, max: 100 },
    { label: "100–300 ₽", min: 100, max: 300 },
    { label: "300–500 ₽", min: 300, max: 500 },
    { label: "500–700 ₽", min: 500, max: 700 },
    { label: "700–1000 ₽", min: 700, max: 1000 },
    { label: "от 1000 ₽", min: 1000, max: Infinity },
  ];
  const MAX_PICK = 12;
  const CHOOSE_TIMEOUT = 40; // сек без выбора → назад к витрине
  const STEP_MS = 1150; // длительность одного шага «сборки»
  const COLLAGE_MS = 3200; // сколько держать готовый коллаж
  const MOSAIC_TILES = 24;

  const pickerEl = document.getElementById("picker");
  const chooseEl = pickerEl.querySelector(".pk-choose");
  const rangesEl = pickerEl.querySelector(".pk-ranges");
  const genEl = pickerEl.querySelector(".pk-gen");
  const mosaicEl = pickerEl.querySelector(".pk-mosaic");
  const stepsEl = pickerEl.querySelector(".pk-steps");
  const progressEl = pickerEl.querySelector(".pk-progress");
  const gridWrap = pickerEl.querySelector(".pk-grid-wrap");
  const gridEl = pickerEl.querySelector(".pk-grid");
  const doneN = pickerEl.querySelector(".pk-done-n");
  const singleWrap = pickerEl.querySelector(".pk-single-wrap");
  const singleEl = pickerEl.querySelector(".pk-single");
  const dotsEl = pickerEl.querySelector(".pk-dots");
  const startBtn = document.querySelector("#cta .picker-btn");
  const codeInput = document.getElementById("code");
  const convertFileSrc = window.__TAURI__.core.convertFileSrc;

  let state = "closed"; // closed | choose | gen | collage | single
  let timers = [];
  let picked = [];
  let pickedRange = null;
  let singleIdx = 0;

  function isPickerKiosk() {
    return setting("kiosk_mode") === "picker";
  }
  function cards() {
    return window.KIOSK_CARDS || [];
  }
  function price(c) {
    return Number(c.discounted_price || c.price) || 0;
  }
  function inRange(c, r) {
    const p = price(c);
    return p > 0 && p >= r.min && p < r.max;
  }
  function shuffle(a) {
    return window.kioskShuffle ? kioskShuffle(a) : a.slice();
  }
  function later(fn, ms) {
    timers.push(setTimeout(fn, ms));
  }
  function clearTimers() {
    timers.forEach(clearTimeout);
    timers = [];
  }
  function plural(n, one, few, many) {
    const m10 = n % 10;
    const m100 = n % 100;
    if (m10 === 1 && m100 !== 11) return one;
    if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return few;
    return many;
  }
  function show(part) {
    chooseEl.hidden = part !== "choose";
    genEl.hidden = part !== "gen";
    gridWrap.hidden = part !== "collage";
    singleWrap.hidden = part !== "single";
  }
  function setRangeLabels(text) {
    pickerEl.querySelectorAll(".pk-range-label").forEach(function (el) {
      el.textContent = text;
    });
  }
  function stopVideos(root) {
    root.querySelectorAll("video").forEach(function (v) {
      v.pause();
      v.removeAttribute("src");
      v.load();
    });
  }

  // ---------- открытие / закрытие ----------

  function openPicker() {
    if (!isPickerKiosk() || SETTINGS_OPENED) return;
    if (!infoEl.hasAttribute("hidden")) return; // идёт показ цены — не мешаем
    pickerEl.hidden = false;
    pickerEl.classList.remove("animate");
    void pickerEl.offsetWidth;
    pickerEl.classList.add("animate");
    showChoose();
  }

  function closePicker() {
    clearTimers();
    state = "closed";
    stopVideos(pickerEl);
    pickerEl.hidden = true;
    mosaicEl.innerHTML = "";
    gridEl.innerHTML = "";
    singleEl.innerHTML = "";
    codeInput.focus();
    window.applyPendingContent && window.applyPendingContent();
  }

  // ---------- 1. выбор суммы ----------

  function showChoose() {
    clearTimers();
    stopVideos(pickerEl);
    state = "choose";
    rangesEl.innerHTML = "";
    RANGES.forEach(function (r, i) {
      const n = cards().filter(function (c) {
        return inRange(c, r);
      }).length;
      const b = document.createElement("button");
      b.type = "button";
      b.className = "pk-range";
      b.innerHTML = '<span class="pk-r-label"></span><span class="pk-r-count"></span>';
      b.querySelector(".pk-r-label").textContent = r.label;
      b.querySelector(".pk-r-count").textContent = n ? n + " " + plural(n, "товар", "товара", "товаров") : "пока нет";
      b.disabled = n === 0;
      b.addEventListener("click", function (e) {
        e.stopPropagation();
        startGeneration(i);
      });
      rangesEl.appendChild(b);
    });
    show("choose");
    later(closePicker, CHOOSE_TIMEOUT * 1000);
    codeInput.focus();
  }

  // ---------- 2. «сборка» подборки ----------

  function startGeneration(i) {
    clearTimers();
    state = "gen";
    pickedRange = RANGES[i];
    const all = cards();
    const matching = all.filter(function (c) {
      return inRange(c, pickedRange);
    });
    if (!matching.length) return showChoose();
    picked = shuffle(matching).slice(0, MAX_PICK);
    setRangeLabels(pickedRange.label);

    // Мозаика: мелькают кадры всех товаров, потом остаются только отобранные.
    mosaicEl.innerHTML = "";
    const pool = shuffle(all);
    const tiles = [];
    for (let k = 0; k < MOSAIC_TILES; k++) {
      const t = document.createElement("div");
      t.className = "pk-m";
      const img = document.createElement("img");
      img.src = convertFileSrc(pool[k % pool.length].path);
      t.appendChild(img);
      t.style.animationDelay = (k % 8) * 0.06 + "s";
      mosaicEl.appendChild(t);
      tiles.push(t);
    }
    let flick = 0;
    function flicker() {
      if (state !== "gen") return;
      for (let n = 0; n < 4; n++) {
        const t = tiles[Math.floor(Math.random() * tiles.length)];
        if (t.classList.contains("keep")) continue;
        t.querySelector("img").src = convertFileSrc(pool[(flick++ * 7 + n) % pool.length].path);
        t.classList.remove("flip");
        void t.offsetWidth;
        t.classList.add("flip");
      }
      later(flicker, 180);
    }
    flicker();

    const total = all.length;
    const steps = [
      "Подключаюсь к серверу ИрсКом",
      "Просматриваю витрину: " + total + " " + plural(total, "товар", "товара", "товаров"),
      "Отбираю по цене " + pickedRange.label + ": найдено " + matching.length,
      "Сверяю цены и наличие в 1С",
      "Монтирую ролики",
      "Собираю вашу подборку",
    ];
    stepsEl.innerHTML = "";
    const rows = steps.map(function (text) {
      const row = document.createElement("div");
      row.className = "pk-step";
      row.innerHTML = '<span class="pk-s-ico"></span><span class="pk-s-text"></span>';
      row.querySelector(".pk-s-text").textContent = text;
      stepsEl.appendChild(row);
      return row;
    });

    const bar = progressEl.querySelector("i");
    const pct = progressEl.querySelector("span");
    bar.style.transition = "none";
    bar.style.width = "0%";
    pct.textContent = "0%";
    show("gen");

    rows.forEach(function (row, idx) {
      later(function () {
        if (idx > 0) rows[idx - 1].classList.replace("run", "ok");
        row.classList.add("run");
        const p = Math.round(((idx + 1) / rows.length) * 100);
        bar.style.transition = "width " + STEP_MS / 1000 + "s linear";
        bar.style.width = p + "%";
        animateNumber(pct, idx === 0 ? 0 : Math.round((idx / rows.length) * 100), p, STEP_MS);
        // на шаге «отбираю» в мозаике подсвечиваются подходящие товары
        if (idx === 2) highlightPicked(tiles);
      }, idx * STEP_MS);
    });
    later(function () {
      rows[rows.length - 1].classList.replace("run", "ok");
      showCollage();
    }, rows.length * STEP_MS + 250);
  }

  function highlightPicked(tiles) {
    const keep = Math.min(tiles.length, picked.length);
    const order = shuffle(tiles.map(function (_, k) {
      return k;
    })).slice(0, keep);
    order.forEach(function (k, j) {
      const t = tiles[k];
      t.querySelector("img").src = convertFileSrc(picked[j].path);
      t.classList.add("keep");
    });
    tiles.forEach(function (t) {
      if (!t.classList.contains("keep")) t.classList.add("drop");
    });
  }

  function animateNumber(el, from, to, ms) {
    const t0 = performance.now();
    function frame(t) {
      const k = Math.min(1, (t - t0) / ms);
      el.textContent = Math.round(from + (to - from) * k) + "%";
      if (k < 1 && state === "gen") requestAnimationFrame(frame);
    }
    requestAnimationFrame(frame);
  }

  // ---------- 3. готовый коллаж ----------

  function tile(card, big) {
    const d = document.createElement("div");
    d.className = big ? "pk-big" : "pk-tile";
    const ph = document.createElement("div");
    ph.className = "pk-photo";
    if (big && card.video_path) {
      const v = document.createElement("video");
      v.src = convertFileSrc(card.video_path);
      v.poster = convertFileSrc(card.path);
      v.muted = true;
      v.loop = true;
      v.autoplay = true;
      v.playsInline = true;
      ph.appendChild(v);
      v.play().catch(function () {});
    } else {
      const img = document.createElement("img");
      img.src = convertFileSrc(card.path);
      ph.appendChild(img);
    }
    const nm = document.createElement("div");
    nm.className = "pk-name";
    nm.textContent = window.kioskCardTitle ? kioskCardTitle(card) : card.name;
    d.appendChild(ph);
    d.appendChild(nm);
    return d;
  }

  function showCollage() {
    clearTimers();
    state = "collage";
    mosaicEl.innerHTML = "";
    gridEl.innerHTML = "";
    gridEl.className = "pk-grid cols-" + (picked.length <= 4 ? 2 : 3);
    doneN.textContent = picked.length + " " + plural(picked.length, "товар", "товара", "товаров") + " для вас";
    picked.forEach(function (c, idx) {
      const t = tile(c, false);
      t.style.animationDelay = idx * 0.07 + "s";
      t.addEventListener("click", function (e) {
        e.stopPropagation();
        startSingles(idx);
      });
      gridEl.appendChild(t);
    });
    show("collage");
    later(function () {
      startSingles(0);
    }, COLLAGE_MS);
  }

  // ---------- 4. по одному ----------

  function startSingles(from) {
    clearTimers();
    state = "single";
    singleIdx = from;
    dotsEl.innerHTML = "";
    picked.forEach(function () {
      dotsEl.appendChild(document.createElement("i"));
    });
    show("single");
    showSingle();
  }

  function showSingle() {
    clearTimers();
    stopVideos(singleEl);
    if (singleIdx >= picked.length) return closePicker();
    singleEl.innerHTML = "";
    singleEl.appendChild(tile(picked[singleIdx], true));
    Array.from(dotsEl.children).forEach(function (d, i) {
      d.className = i === singleIdx ? "on" : "";
    });
    const secs = Math.max(4, Number(setting("slider_interval")) || 6);
    later(function () {
      singleIdx++;
      showSingle();
    }, secs * 1000);
    codeInput.focus();
  }

  // ---------- управление ----------

  // Касание по товару при показе по одному — следующий.
  singleWrap.addEventListener("click", function (e) {
    e.stopPropagation();
    singleIdx++;
    showSingle();
  });

  // Касание мимо сумм на экране выбора — назад к витрине.
  pickerEl.addEventListener("click", function () {
    if (state === "choose") closePicker();
  });

  // Кнопка на экране.
  startBtn &&
    startBtn.addEventListener("click", function (e) {
      e.stopPropagation();
      openPicker();
    });

  // Необязательная внешняя кнопка (приходит как нажатие клавиши, F3).
  document.addEventListener(
    "keydown",
    function (e) {
      if (!isPickerKiosk() || SETTINGS_OPENED) return;
      const key = setting("picker_key");
      if (!key || e.code !== key) return;
      // Сканер штрихкода заканчивает ввод Enter — отличаем по пустому полю.
      if ((key === "Enter" || key === "NumpadEnter") && codeInput.value) return;
      e.preventDefault();
      e.stopPropagation();
      if (state === "closed" || state === "single" || state === "collage") openPicker();
    },
    true
  );

  // На сенсорном экране два быстрых касания = dblclick, а он в киоске
  // выходит из полноэкранного режима. На киоске подбора это не нужно.
  ["picker", "waiting"].forEach(function (id) {
    document.getElementById(id).addEventListener("dblclick", function (e) {
      if (isPickerKiosk()) e.stopPropagation();
    });
  });

  // Отсканировали товар — подбор закрываем, работает обычная проверка цены.
  codeInput.addEventListener("change", function () {
    if (state !== "closed") closePicker();
  });

  window.isPickerOpen = function () {
    return state !== "closed";
  };

  window.refreshPickerUI = function () {
    if (startBtn) startBtn.hidden = !isPickerKiosk();
    document.getElementById("waiting").classList.toggle("picker-mode", isPickerKiosk());
    if (!isPickerKiosk() && state !== "closed") closePicker();
  };

  window.refreshPickerUI();
  // настройки из settings.json приходят асинхронно — обновим ещё раз
  setTimeout(window.refreshPickerUI, 1500);
})();
