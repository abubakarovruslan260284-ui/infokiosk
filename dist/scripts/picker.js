// Подборка по цене — только на киоске в режиме "picker" (F3 → Режим
// киоска). Посетитель жмёт USB-кнопку (или касается экрана) → выбирает
// сумму → киоск показывает плиткой 8–12 товаров из этого диапазона →
// потом листает их по одному крупно → возвращается к витрине.
// Цены у товаров здесь НЕ показываются — только диапазон в заголовке.
//
// Товары берутся из тех же карточек, что и витрина (window.KIOSK_CARDS,
// заполняет content.js из локального кэша). Диапазон определяется по
// цене по карте.

(function () {
  const RANGES = [
    { label: "до 100 ₽", min: 0, max: 100 },
    { label: "100–300 ₽", min: 100, max: 300 },
    { label: "300–500 ₽", min: 300, max: 500 },
    { label: "500–700 ₽", min: 500, max: 700 },
    { label: "700–1000 ₽", min: 700, max: 1000 },
    { label: "от 1000 ₽", min: 1000, max: Infinity },
  ];
  const MAX_TILES = 12;
  const CHOOSE_TIMEOUT = 30; // сек без выбора → назад к витрине
  const GRID_SECONDS = 12; // сколько держать плитку перед показом по одному

  const pickerEl = document.getElementById("picker");
  const chooseEl = pickerEl.querySelector(".pk-choose");
  const rangesEl = pickerEl.querySelector(".pk-ranges");
  const gridWrap = pickerEl.querySelector(".pk-grid-wrap");
  const gridEl = pickerEl.querySelector(".pk-grid");
  const singleWrap = pickerEl.querySelector(".pk-single-wrap");
  const singleEl = pickerEl.querySelector(".pk-single");
  const dotsEl = pickerEl.querySelector(".pk-dots");
  const hintEl = document.querySelector("#cta .picker-hint");
  const codeInput = document.getElementById("code");
  const convertFileSrc = window.__TAURI__.core.convertFileSrc;

  let state = "closed"; // closed | choose | grid | single
  let timer = null;
  let picked = [];
  let pickedRange = null;
  let singleIdx = 0;

  function isPickerKiosk() {
    return setting("kiosk_mode") === "picker";
  }

  function price(c) {
    return Number(c.discounted_price || c.price) || 0;
  }

  function inRange(c, r) {
    const p = price(c);
    return p > 0 && p >= r.min && p < r.max;
  }

  function clearTimer() {
    timer && clearTimeout(timer);
    timer = null;
  }

  function show(part) {
    chooseEl.hidden = part !== "choose";
    gridWrap.hidden = part !== "grid";
    singleWrap.hidden = part !== "single";
  }

  function openPicker() {
    if (!isPickerKiosk() || SETTINGS_OPENED) return;
    // во время карточки цены кнопку не перехватываем — дождёмся возврата
    if (!infoEl.hasAttribute("hidden")) return;
    pickerEl.hidden = false;
    pickerEl.classList.remove("animate");
    void pickerEl.offsetWidth;
    pickerEl.classList.add("animate");
    showChoose();
  }

  function closePicker() {
    clearTimer();
    state = "closed";
    pickerEl.hidden = true;
    gridEl.innerHTML = "";
    singleEl.innerHTML = "";
    codeInput.focus();
    window.applyPendingContent && window.applyPendingContent();
  }

  function showChoose() {
    clearTimer();
    state = "choose";
    const cards = window.KIOSK_CARDS || [];
    rangesEl.innerHTML = "";
    RANGES.forEach(function (r, i) {
      const n = cards.filter(function (c) {
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
        chooseRange(i);
      });
      rangesEl.appendChild(b);
    });
    show("choose");
    timer = setTimeout(closePicker, CHOOSE_TIMEOUT * 1000);
    codeInput.focus();
  }

  function plural(n, one, few, many) {
    const m10 = n % 10;
    const m100 = n % 100;
    if (m10 === 1 && m100 !== 11) return one;
    if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return few;
    return many;
  }

  function tile(card, big) {
    const d = document.createElement("div");
    d.className = big ? "pk-big" : "pk-tile";
    const ph = document.createElement("div");
    ph.className = "pk-photo";
    const img = document.createElement("img");
    img.src = convertFileSrc(card.path);
    ph.appendChild(img);
    const nm = document.createElement("div");
    nm.className = "pk-name";
    nm.textContent = window.kioskCardTitle ? kioskCardTitle(card) : card.name;
    d.appendChild(ph);
    d.appendChild(nm);
    return d;
  }

  function setRangeLabels(text) {
    pickerEl.querySelectorAll(".pk-range-label").forEach(function (el) {
      el.textContent = text;
    });
  }

  function chooseRange(i) {
    clearTimer();
    pickedRange = RANGES[i];
    const all = (window.KIOSK_CARDS || []).filter(function (c) {
      return inRange(c, pickedRange);
    });
    if (!all.length) return showChoose();
    picked = (window.kioskShuffle ? kioskShuffle(all) : all).slice(0, MAX_TILES);
    setRangeLabels(pickedRange.label);

    gridEl.innerHTML = "";
    gridEl.className = "pk-grid cols-" + (picked.length <= 4 ? 2 : picked.length <= 8 ? 2 : 3);
    picked.forEach(function (c, idx) {
      const t = tile(c, false);
      t.style.animationDelay = idx * 0.05 + "s";
      t.addEventListener("click", function (e) {
        e.stopPropagation();
        startSingles(idx);
      });
      gridEl.appendChild(t);
    });
    state = "grid";
    show("grid");
    timer = setTimeout(function () {
      startSingles(0);
    }, GRID_SECONDS * 1000);
    codeInput.focus();
  }

  function startSingles(from) {
    clearTimer();
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
    clearTimer();
    if (singleIdx >= picked.length) return closePicker();
    singleEl.innerHTML = "";
    singleEl.appendChild(tile(picked[singleIdx], true));
    Array.from(dotsEl.children).forEach(function (d, i) {
      d.className = i === singleIdx ? "on" : "";
    });
    const secs = Math.max(4, Number(setting("slider_interval")) || 6);
    timer = setTimeout(function () {
      singleIdx++;
      showSingle();
    }, secs * 1000);
    codeInput.focus();
  }

  // Касание по одиночной карточке — следующая.
  singleWrap.addEventListener("click", function (e) {
    e.stopPropagation();
    singleIdx++;
    showSingle();
  });

  // Касание по пустому месту подборки на экране выбора — назад к витрине.
  pickerEl.addEventListener("click", function () {
    if (state === "choose") closePicker();
  });

  // USB-кнопка (она приходит в киоск как нажатие клавиши).
  document.addEventListener(
    "keydown",
    function (e) {
      if (!isPickerKiosk() || SETTINGS_OPENED) return;
      const key = setting("picker_key");
      if (!key || e.code !== key) return;
      // Сканер штрихкода тоже заканчивает ввод клавишей Enter — если
      // кнопка настроена на Enter, отличаем её по пустому полю ввода.
      if ((key === "Enter" || key === "NumpadEnter") && codeInput.value) return;
      e.preventDefault();
      e.stopPropagation();
      openPicker();
    },
    true
  );

  // Касание экрана ожидания на киоске с подборкой — тоже открывает выбор.
  document.getElementById("waiting").addEventListener("click", function () {
    if (isPickerKiosk() && state === "closed") openPicker();
  });

  // На сенсорном экране два быстрых касания = dblclick, а он в киоске
  // выходит из полноэкранного режима. В зонах подборки это не нужно.
  ["picker", "slider"].forEach(function (id) {
    document.getElementById(id).addEventListener("dblclick", function (e) {
      if (isPickerKiosk()) e.stopPropagation();
    });
  });

  // Отсканировали товар — подборку закрываем, работает обычная проверка цены.
  codeInput.addEventListener("change", function () {
    if (state !== "closed") closePicker();
  });

  window.isPickerOpen = function () {
    return state !== "closed";
  };

  window.refreshPickerUI = function () {
    if (hintEl) hintEl.hidden = !isPickerKiosk();
    if (!isPickerKiosk() && state !== "closed") closePicker();
  };

  window.refreshPickerUI();
  // настройки из settings.json приходят асинхронно — обновим подсказку ещё раз
  setTimeout(window.refreshPickerUI, 1500);
})();
