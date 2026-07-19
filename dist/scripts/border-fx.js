// Подсветка рамки вокруг слайдов ("для привлечения внимания покупателя").
// Настраивается через F3 → поля "Подсветка рамки" (off / solid / rainbow),
// "Цвет", "Скорость переливания" и "Интенсивность свечения". Читает
// значения напрямую из глобального APP_SETTINGS (его создаёт settings.js).
// settings.js вызывает window.setUISettings после каждого сохранения
// настроек, поэтому изменения применяются сразу, без перезапуска.
(function () {
  const glowEl = document.getElementById("slider-glow");

  function num(v, def) {
    const n = parseFloat(String(v).replace(",", "."));
    return isNaN(n) ? def : n;
  }

  function applyBorderFx() {
    const S = typeof APP_SETTINGS !== "undefined" && APP_SETTINGS ? APP_SETTINGS : {};

    const rawMode = S["border_mode"] || "rainbow";
    const mode = ["off", "solid", "rainbow"].includes(rawMode) ? rawMode : "rainbow";

    const color = S["border_color"] || "#e73a7c";
    const speed = num(S["border_speed"], 6) || 6;

    let intensity = num(S["border_intensity"], 0.7);
    intensity = Math.min(1, Math.max(0, intensity));

    glowEl.classList.remove("mode-off", "mode-solid", "mode-rainbow");
    glowEl.classList.add("mode-" + mode);
    glowEl.style.setProperty("--glow-speed", speed + "s");
    glowEl.style.setProperty(
      "--glow-color",
      /^#[0-9a-fA-F]{3,8}$/.test(color) ? color : "#e73a7c"
    );

    // Интенсивность = непрозрачность свечения (0..1). В режиме "off" —
    // убираем inline-значение, чтобы сработал базовый opacity:0 из CSS.
    glowEl.style.opacity = mode === "off" ? "" : String(intensity);
  }

  // Точка расширения, которую settings.js вызывает сам после Сохранить/Импорт.
  window.setUISettings = applyBorderFx;

  applyBorderFx();
})();
