// Подсветка рамки вокруг слайдов ("для привлечения внимания покупателя").
// Настраивается через F3 → поля "Подсветка рамки" (off / solid / rainbow),
// "Цвет" и "Скорость переливания". Читает значения напрямую из глобального
// APP_SETTINGS (его создаёт settings.js) — settings.js при этом остаётся
// НЕ ТРОНУТЫМ: он уже предусматривает точку расширения window.setUISettings,
// которую settings.js сам вызывает после каждого сохранения настроек.
(function () {
  const glowEl = document.getElementById("slider-glow");

  function applyBorderFx() {
    const raw = (typeof APP_SETTINGS !== "undefined" && APP_SETTINGS["border_mode"]) || "rainbow";
    const mode = ["off", "solid", "rainbow"].includes(raw) ? raw : "rainbow";
    const color = (typeof APP_SETTINGS !== "undefined" && APP_SETTINGS["border_color"]) || "#e73a7c";
    const speedRaw = typeof APP_SETTINGS !== "undefined" ? APP_SETTINGS["border_speed"] : "";
    const speed = parseFloat(String(speedRaw).replace(",", ".")) || 6;

    glowEl.classList.remove("mode-off", "mode-solid", "mode-rainbow");
    glowEl.classList.add("mode-" + mode);
    glowEl.style.setProperty("--glow-speed", speed + "s");
    glowEl.style.setProperty("--glow-color", /^#[0-9a-fA-F]{3,8}$/.test(color) ? color : "#e73a7c");
  }

  // Точка расширения, которую settings.js вызывает сам после Сохранить/Импорт.
  window.setUISettings = applyBorderFx;

  applyBorderFx();
})();
