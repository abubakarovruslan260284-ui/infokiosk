// Диагностическая панель — F4. Не имеет отношения к ценам/1С и никак
// не завязана на script.js/settings.js. Показывает реальное состояние
// с диска: версию сборки, путь к папке контента, сколько слайдов в
// кэше сейчас и последние строки sync.log — чтобы не гадать, а видеть.
(function () {
  const invoke = window.__TAURI__.core.invoke;

  const panel = document.createElement("div");
  panel.id = "diag-panel";
  panel.style.cssText = [
    "position:fixed", "inset:4vh 4vw", "z-index:999",
    "background:rgba(10,14,20,.96)", "color:#d7e2ee",
    "font-family:Consolas,Menlo,monospace", "font-size:15px", "line-height:1.55",
    "border-radius:14px", "padding:28px 32px", "overflow:auto",
    "box-shadow:0 20px 60px rgba(0,0,0,.6)", "white-space:pre-wrap",
    "display:none",
  ].join(";");
  document.body.appendChild(panel);

  function closeBtn() {
    return '<div style="position:absolute;top:14px;right:18px;cursor:pointer;font-size:22px;color:#88a" onclick="document.getElementById(\'diag-panel\').style.display=\'none\'">✕</div>';
  }

  async function render() {
    let d;
    try {
      d = await invoke("read_diagnostics");
    } catch (e) {
      panel.innerHTML = closeBtn() + "Не удалось получить диагностику: " + e;
      return;
    }
    panel.innerHTML =
      closeBtn() +
      "<div style='font-size:20px;color:#fff;margin-bottom:14px'>Диагностика инфокиоска — F4 закрыть/открыть</div>" +
      "<div><b>Версия сборки:</b> " + esc(d.app_version) + "</div>" +
      "<div><b>Папка контента (content_source_path):</b> " + esc(d.content_source_path) + "</div>" +
      "<div><b>Файл настроек:</b> " + esc(d.settings_path) + "</div>" +
      "<div><b>Локальный кэш:</b> " + esc(d.cache_root) + "</div>" +
      "<div><b>Слайдов в кэше сейчас:</b> " + d.slide_count + "</div>" +
      "<div style='margin-top:16px;color:#fff'><b>Последние записи журнала синхронизации:</b></div>" +
      "<div style='margin-top:6px;color:#9fb'>" + esc(d.log_tail) + "</div>" +
      "<div style='margin-top:20px;color:#789'>Обновить: F4 закрыть и снова открыть. Форсировать синхронизацию сейчас: F5.</div>";
  }

  function esc(s) {
    return String(s).replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));
  }

  document.addEventListener("keydown", function (e) {
    if (e.key === "F4") {
      const showing = panel.style.display !== "none";
      if (showing) {
        panel.style.display = "none";
      } else {
        panel.style.display = "block";
        render();
      }
    }
    if (e.key === "F5") {
      invoke("force_sync").then(function (r) {
        if (panel.style.display !== "none") render();
      }).catch(function () {});
    }
  });
})();
