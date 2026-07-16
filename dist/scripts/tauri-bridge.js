// Мост совместимости: даёт скриптам settings.js / script.js тот же
// window.electronAPI, что был в Electron-версии, но за ним — вызовы
// Tauri. Благодаря этому сами скрипты (та часть, что отвечает за
// сканирование штрихкода и получение цены из 1С — «скелет») остаются
// НЕ ТРОНУТЫМИ при переезде с Electron на Tauri.
(function () {
  const invoke = window.__TAURI__.core.invoke;

  window.electronAPI = {
    exitFullscreen: () => invoke("exit_fullscreen"),
    saveSettings: (data) => invoke("save_settings_dialog", { data: toRustSettings(data) }),
    loadSettings: () => invoke("load_settings_dialog"),
    settingsFromFile: async () => {
      const data = await invoke("get_settings");
      return { data: fromRustSettings(data), settingsFilePath: "settings.json" };
    },
    getLocalSlides: () => invoke("list_active_slides"),
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
    };
  }
})();
