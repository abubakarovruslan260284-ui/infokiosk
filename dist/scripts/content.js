// Слайды берутся из локального кэша (см. kiosk_sync на стороне Rust —
// он уже заранее скачал и проверил файлы; здесь сеть не участвует).
//
// Два принципа, которые прямо отвечают на «чтобы не подтормаживало»:
//   1) Все картинки/видео заранее ПОЛНОСТЬЮ загружаются в скрытые
//      элементы и декодируются браузером ДО того, как их покажут.
//      Показ — это просто смена класса/видимости уже готового узла,
//      без построения разметки и без сетевых или файловых операций
//      в момент показа.
//   2) Обновление контента (когда «Издатель» опубликовал новое) НИКОГДА
//      не подменяет то, что видно на экране прямо сейчас — оно готовит
//      новый набор в фоне и подменяет DOM только в безопасный момент
//      (когда открыт именно экран ожидания, а не карточка цены).

const sliderListEl = document.querySelector("#slider ul");
const invoke = window.__TAURI__.core.invoke;
const convertFileSrc = window.__TAURI__.core.convertFileSrc;
const tauriEvent = window.__TAURI__.event;

let pendingRefresh = false;

async function loadSlides(opts) {
  opts = opts || {};
  let slides;
  try {
    slides = await invoke("list_active_slides");
  } catch (e) {
    console.warn("Не удалось получить список слайдов:", e);
    return;
  }
  if (!slides || slides.length === 0) return;

  const infoVisible = !infoEl.hasAttribute("hidden");
  if (infoVisible && !opts.force) {
    pendingRefresh = true;
    return;
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

loadSlides({ force: true });
