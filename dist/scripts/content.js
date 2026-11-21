// Контент экрана ожидания: рекламные слайды + карточки товаров.
// Всё берётся из ЛОКАЛЬНОГО кэша (см. kiosk_sync на стороне Rust — он
// заранее скачал и проверил файлы; сеть здесь не участвует).
//
// Принципы «чтобы не подтормаживало»:
//   1) Следующие слайды заранее загружаются и декодируются ДО показа.
//   2) Обновление контента не подменяет то, что видно на экране прямо
//      сейчас — только в безопасный момент (экран ожидания).
//   3) Карточек товаров могут быть сотни. Держать их все в памяти
//      слабому киоску нельзя, поэтому в DOM живут только текущий слайд
//      и пара следующих; дальше они подгружаются по ходу показа.
//
// Карусель: карточки (ролики товаров) в случайном порядке — у каждого
// киоска свой, и на каждом новом круге порядок перемешивается заново.
// После каждых трёх карточек — один рекламный слайд, если реклама есть.

const sliderListEl = document.querySelector("#slider ul");
const invoke = window.__TAURI__.core.invoke;
const convertFileSrc = window.__TAURI__.core.convertFileSrc;
const tauriEvent = window.__TAURI__.event;

const AHEAD = 2; // сколько следующих слайдов держать готовыми
const STATIC_LIMIT = 4; // до стольких слайдов — все сразу в DOM, как раньше

let pendingRefresh = false;
let playlist = [];
let playPos = 0;
let windowed = false;

// Карточки для режима подборки (picker.js) — независимо от того,
// показываются ли они в карусели.
window.KIOSK_CARDS = [];

function shuffle(arr) {
  const a = arr.slice();
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    const t = a[i];
    a[i] = a[j];
    a[j] = t;
  }
  return a;
}
window.kioskShuffle = shuffle;

function buildPlaylist(slides, cards) {
  const ads = slides.map(function (s) {
    return { type: s.kind, path: s.path };
  });
  const cs = shuffle(cards).map(function (c) {
    return { type: "card", card: c };
  });
  if (!cs.length) return ads;
  if (!ads.length) return cs;
  const out = [];
  let a = 0;
  cs.forEach(function (c, i) {
    if (i % 3 === 0) {
      out.push(ads[a % ads.length]);
      a++;
    }
    out.push(c);
  });
  while (a < ads.length) out.push(ads[a++]);
  return out;
}

function splitPrice(value) {
  const v = Math.round(Number(value) * 100) / 100;
  const rub = Math.floor(v);
  const kop = Math.round((v - rub) * 100);
  return { rub: String(rub).replace(/\B(?=(\d{3})+(?!\d))/g, " "), kop: kop < 10 ? "0" + kop : String(kop) };
}
window.kioskSplitPrice = splitPrice;

function cardTitle(card) {
  return [card.name, card.characteristic].filter(Boolean).join(", ");
}
window.kioskCardTitle = cardTitle;

/** Создаёт <li> слайда и обещание «готов к показу» (картинка декодирована). */
function makeSlide(item) {
  const li = document.createElement("li");
  let ready;

  if (item.type === "video") {
    const video = document.createElement("video");
    video.src = convertFileSrc(item.path);
    video.muted = true;
    video.loop = true;
    video.playsInline = true;
    video.preload = "auto";
    li.appendChild(video);
    ready = new Promise(function (resolve) {
      video.addEventListener("canplaythrough", resolve, { once: true });
      video.addEventListener("error", resolve, { once: true });
      setTimeout(resolve, 8000);
    });
  } else if (item.type === "card") {
    const c = item.card;
    li.className = "card-slide";
    const photo = document.createElement("div");
    photo.className = "cs-photo";
    let mediaReady;
    if (c.video_path) {
      // Ролик товара: крутится по кругу без звука, пока карточка на экране
      // (запуск/пауза — attachVideoAutoplay ниже).
      const v = document.createElement("video");
      v.src = convertFileSrc(c.video_path);
      v.poster = convertFileSrc(c.path);
      v.muted = true;
      v.loop = true;
      v.playsInline = true;
      v.preload = "auto";
      photo.appendChild(v);
      mediaReady = new Promise(function (resolve) {
        v.addEventListener("canplaythrough", resolve, { once: true });
        v.addEventListener("error", resolve, { once: true });
        setTimeout(resolve, 6000);
      });
    } else {
      const img = document.createElement("img");
      img.src = convertFileSrc(c.path);
      photo.appendChild(img);
      mediaReady = (img.decode ? img.decode() : Promise.resolve()).catch(function () {});
    }

    const info = document.createElement("div");
    info.className = "cs-info";
    const name = document.createElement("div");
    name.className = "cs-name";
    name.textContent = cardTitle(c);
    const price = document.createElement("div");
    price.className = "cs-price";
    const p = splitPrice(c.discounted_price || c.price);
    price.innerHTML =
      '<span class="cs-value"></span><span class="cs-tail"><span class="cs-kop"></span><i>&#8381;</i></span>';
    price.querySelector(".cs-value").textContent = p.rub;
    price.querySelector(".cs-kop").textContent = p.kop;
    const label = document.createElement("div");
    label.className = "cs-label";
    label.textContent = "Цена по карте ИрсКом";
    info.appendChild(name);
    info.appendChild(price);
    info.appendChild(label);

    li.appendChild(photo);
    li.appendChild(info);
    ready = mediaReady;
  } else {
    const img = document.createElement("img");
    img.src = convertFileSrc(item.path);
    li.appendChild(img);
    ready = (img.decode ? img.decode() : Promise.resolve()).catch(function () {});
  }
  return { li: li, ready: ready };
}

function disposeSlide(li) {
  li.querySelectorAll("video").forEach(function (v) {
    v.pause();
    v.removeAttribute("src");
    v.load();
  });
  li.remove();
}

let lastSlides = [];
let lastCards = [];

function nextItem() {
  // Новый круг — новый случайный порядок (чтобы киоск не повторял одну и
  // ту же последовательность весь день).
  if (playPos > 0 && playPos % playlist.length === 0 && windowed) {
    playlist = buildPlaylist(lastSlides, lastCards);
  }
  const it = playlist[playPos % playlist.length];
  playPos++;
  return it;
}

function screenBusy() {
  const infoVisible = !infoEl.hasAttribute("hidden");
  return infoVisible || (window.isPickerOpen && window.isPickerOpen());
}

/** @returns {Promise<boolean>} true, если есть что показывать */
async function loadSlides(opts) {
  opts = opts || {};
  let slides = [];
  let cards = [];
  try {
    slides = (await invoke("list_active_slides")) || [];
  } catch (e) {
    console.warn("Не удалось получить список слайдов:", e);
  }
  try {
    cards = (await invoke("list_cards")) || [];
  } catch (e) {
    console.warn("Не удалось получить карточки:", e);
  }
  window.KIOSK_CARDS = cards;

  const carouselCards = setting("show_cards") ? cards : [];
  if (!slides.length && !carouselCards.length) return false;

  if (screenBusy() && !opts.force) {
    pendingRefresh = true;
    return true; // контент ЕСТЬ, применим его чуть позже
  }

  lastSlides = slides;
  lastCards = carouselCards;
  playlist = buildPlaylist(slides, carouselCards);
  playPos = 0;
  windowed = playlist.length > STATIC_LIMIT;

  const count = windowed ? AHEAD + 1 : playlist.length;
  const prepared = [];
  for (let i = 0; i < count; i++) prepared.push(makeSlide(nextItem()));
  await Promise.all(
    prepared.map(function (p) {
      return p.ready;
    })
  );

  Array.from(sliderListEl.children).forEach(disposeSlide);
  prepared.forEach(function (p) {
    sliderListEl.appendChild(p.li);
  });
  sliderListEl.querySelectorAll("video").forEach(function (v) {
    v.pause();
  });

  window.restartSlider && restartSlider();
  pendingRefresh = false;
  attachVideoAutoplay();
  return true;
}

// Вызывается слайдером после каждого перелистывания.
window.sliderAfterAdvance = function () {
  if (!windowed || !playlist.length) return;
  const items = Array.from(sliderListEl.children);
  const enterIdx = items.findIndex(function (li) {
    return li.classList.contains("enter");
  });
  if (enterIdx < 0) return;
  // всё, что старше уходящего слайда, больше не нужно
  for (let i = 0; i < enterIdx - 1; i++) disposeSlide(items[i]);
  // впереди всегда AHEAD готовящихся слайдов
  let ahead = sliderListEl.children.length - 1 - Array.from(sliderListEl.children).indexOf(items[enterIdx]);
  while (ahead < AHEAD) {
    sliderListEl.appendChild(makeSlide(nextItem()).li);
    ahead++;
  }
};

let videoObserver = null;
function attachVideoAutoplay() {
  videoObserver && videoObserver.disconnect();
  videoObserver = new MutationObserver(function () {
    sliderListEl.querySelectorAll("li").forEach(function (li) {
      const video = li.querySelector("video");
      if (!video) return;
      if (li.classList.contains("enter")) {
        if (video.paused) {
          video.currentTime = 0;
          video.play().catch(function () {});
        }
      } else {
        video.pause();
      }
    });
  });
  videoObserver.observe(sliderListEl, { attributes: true, attributeFilter: ["class"], subtree: true, childList: true });
}

tauriEvent.listen("kiosk://content-updated", function () {
  loadSlides({});
});

// Настройки изменились (например, включили/выключили карточки).
window.reloadContent = function () {
  loadSlides({});
};

// Применить отложенное обновление, когда вернулись на экран ожидания
// (после карточки цены или после подборки).
window.applyPendingContent = function () {
  if (pendingRefresh) {
    setTimeout(function () {
      loadSlides({ force: true });
    }, 750);
  }
};

const origSwitchToWaiting = window.switchToWaiting;
if (typeof origSwitchToWaiting === "function") {
  window.switchToWaiting = function () {
    origSwitchToWaiting.apply(this, arguments);
    window.applyPendingContent();
  };
}

// Настойчивая стартовая загрузка: пробуем сразу, и если не вышло —
// повторяем раз в 5 секунд (до ~3.5 минут), пока не получится хотя бы раз.
(async function retryUntilFirstSuccess() {
  const ok = await loadSlides({ force: true });
  if (ok) return;
  let attempts = 0;
  const timer = setInterval(async function () {
    attempts++;
    const success = await loadSlides({ force: true });
    if (success || attempts > 40) clearInterval(timer);
  }, 5000);
})();
