// Слайды берутся из локального кэша (см. kiosk_sync на стороне Rust —
// он уже заранее скачал и проверил файлы; здесь сеть не участвует).
//
// Два принципа, которые прямо отвечают на «чтобы не подтормаживало»:
//   1) Все картинки/видео заранее ПОЛНОСТЬЮ загружаются в скрытые
//      элементы и декодируются браузером ДО того, как их покажут.
//   2) Обновление контента НИКОГДА не подменяет то, что видно на экране
//      прямо сейчас — подмена DOM только в безопасный момент (экран
//      ожидания, а не карточка цены).
//
// Плюс: стартовая загрузка настойчива. Кэш на свежем киоске может быть
// ещё пуст в момент открытия страницы (первая синхронизация не успела
// завершиться) — поэтому не сдаёмся после одной неудачи, а пробуем
// каждые несколько секунд, пока не получим реальные слайды хотя бы раз.

const sliderListEl = document.querySelector("#slider ul");
const invoke = window.__TAURI__.core.invoke;
const convertFileSrc = window.__TAURI__.core.convertFileSrc;
const tauriEvent = window.__TAURI__.event;

let pendingRefresh = false;

/** @returns {Promise<boolean>} true, если реальные слайды из кэша были показаны */
async function loadSlides(opts) {
  opts = opts || {};
  let slides;
  try {
    slides = await invoke("list_active_slides");
  } catch (e) {
    console.warn("Не удалось получить список слайдов:", e);
    return false;
  }
  if (!slides || slides.length === 0) return false;

  const infoVisible = !infoEl.hasAttribute("hidden");
  if (infoVisible && !opts.force) {
    pendingRefresh = true;
    return true; // слайды ЕСТЬ, просто применим их чуть позже
  }

  const fragment = document.createDocumentFragment();
  const preloadPromises = [];

  for (const slide of slides) {
    const li = document.createElement("li");
    const url = convertFileSrc(slide.path);

    if (slide.kind === "video") {
      const video = document.createElement("video");
      video.src = url;
      video.muted = true;
      video.loop = true;
      video.playsInline = true;
      video.preload = "auto";
      li.appendChild(video);
      preloadPromises.push(
        new Promise((resolve) => {
          video.addEventListener("canplaythrough", resolve, { once: true });
          video.addEventListener("error", resolve, { once: true });
        })
      );
    } else {
      const img = document.createElement("img");
      img.src = url;
      li.appendChild(img);
      preloadPromises.push((img.decode ? img.decode() : Promise.resolve()).catch(function () {}));
    }
    fragment.appendChild(li);
  }

  await Promise.all(preloadPromises);

  sliderListEl.innerHTML = "";
  sliderListEl.appendChild(fragment);
  sliderListEl.querySelectorAll("video").forEach(function (v) { v.pause(); });

  window.restartSlider && restartSlider();
  pendingRefresh = false;
  attachVideoAutoplay();
  return true;
}

let videoObserver = null;
function attachVideoAutoplay() {
  videoObserver && videoObserver.disconnect();
  videoObserver = new MutationObserver(function () {
    sliderListEl.querySelectorAll("li").forEach(function (li) {
      const video = li.querySelector("video");
      if (!video) return;
      if (li.classList.contains("enter")) {
        video.currentTime = 0;
        video.play().catch(function () {});
      } else {
        video.pause();
      }
    });
  });
  videoObserver.observe(sliderListEl, { attributes: true, attributeFilter: ["class"], subtree: true });
}

tauriEvent.listen("kiosk://content-updated", function () {
  loadSlides({});
});

const origSwitchToWaiting = window.switchToWaiting;
if (typeof origSwitchToWaiting === "function") {
  window.switchToWaiting = function () {
    origSwitchToWaiting.apply(this, arguments);
    if (pendingRefresh) {
      setTimeout(function () { loadSlides({ force: true }); }, 750);
    }
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
