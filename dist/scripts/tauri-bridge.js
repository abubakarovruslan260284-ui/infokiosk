// Мост совместимости: даёт скриптам settings.js / script.js тот же
// window.electronAPI, что был в Electron-версии, но за ним — вызовы
// Tauri. Скрипты сканирования/цены (скелет) остаются НЕ ТРОНУТЫМИ.
(function () {
  const invoke = window.__TAURI__.core.invoke;
  window.electronAPI = {
    exitFullscreen: () => invoke("toggle_window_mode"),
    toggleWindowMode: () => invoke("toggle_window_mode"),
    saveSettings: (data) => invoke("save_settings_dialog", { data: toRustSettings(data) }),
    persistSettings: (data) => invoke("save_settings", { data: toRustSettings(data) }),
    // Файл настроек хранится в «формате Rust» (content_source_path,
    // slide_seconds…), а форма ждёт свои имена полей — переводим, иначе
    // при импорте терялись путь к контенту и интервал слайдера.
    loadSettings: async () => {
      const data = await invoke("load_settings_dialog");
      if (data && typeof data === "object" && "content_source_path" in data) return fromRustSettings(data);
      return data;
    },
    settingsFromFile: async () => {
      const data = await invoke("get_settings");
      return { data: fromRustSettings(data), settingsFilePath: "settings.json" };
    },
    getLocalSlides: () => invoke("list_active_slides"),
    getCards: () => invoke("list_cards"),
    forceSync: () => invoke("force_sync"),
  };
  function fromRustSettings(s) {
    if (!s) return s;
    return {
      url_products: s.url_products,
      url_promo: s.content_source_path,
      login: s.login,
      password: s.password,
      show_logo: s.show_logo,
      slider_interval: s.slide_seconds,
      // Поля подсветки рамки (F3) — раньше мост их не пробрасывал,
      // поэтому при старте они не доходили до APP_SETTINGS и рамка
      // всегда возвращалась к дефолту после перезапуска.
      border_mode: s.border_mode,
      border_color: s.border_color,
      border_speed: s.border_speed_sec,
      border_intensity: s.border_intensity,
      kiosk_mode: s.kiosk_mode,
      picker_key: s.picker_key,
      show_cards: s.show_cards,
    };
  }
  function toRustSettings(s) {
    return {
      url_products: s.url_products || "",
      login: s.login || "",
      password: s.password || "",
      content_source_path: s.url_promo || "",
      sync_poll_secs: 20,
      slide_seconds: Number(s.slider_interval) || 6,
      show_logo: !!s.show_logo,
      // Отправляем настройки рамки обратно в Rust, чтобы «Сохранить»
      // их реально сохранял, а не сбрасывал.
      border_mode: s.border_mode || "rainbow",
      border_color: s.border_color || "#e73a7c",
      border_speed_sec: Number(s.border_speed) || 6,
      border_intensity: Number(s.border_intensity) != null && !isNaN(Number(s.border_intensity)) ? Number(s.border_intensity) : 0.7,
      kiosk_mode: s.kiosk_mode === "picker" ? "picker" : "showcase",
      picker_key: s.picker_key || "F9",
      show_cards: !(s.show_cards === false || s.show_cards === "false"),
    };
  }
})();
