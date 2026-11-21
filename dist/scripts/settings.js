/*** UTILS ***/

// Все настройки в одном виде — и для сохранения в settings.json, и для
// кнопки «Экспорт» (раньше экспорт терял рамку и путь к контенту).
function collectSettings() {
  return {
    url_products: setting("api_url"),
    url_promo: setting("api_url_2"),
    login: setting("login"),
    password: setting("password"),
    show_logo: setting("show_logo"),
    slider_interval: setting("slider_interval"),
    border_mode: setting("border_mode"),
    border_color: setting("border_color"),
    border_speed: setting("border_speed"),
    border_intensity: setting("border_intensity"),
    kiosk_mode: setting("kiosk_mode"),
    picker_key: setting("picker_key"),
    show_cards: setting("show_cards"),
  };
}

function applySettings() {
  localStorage.setItem("settings", JSON.stringify(APP_SETTINGS));

  // Пробрасываем настройки в Rust (рабочий settings.json + живой процесс),
  // чтобы путь к папке контента и интервал слайдера реально применялись к
  // фоновой синхронизации, а не оставались только в localStorage.
  window.electronAPI &&
    window.electronAPI.persistSettings &&
    window.electronAPI.persistSettings(collectSettings());

  initAuthToken();
  window.reInitContentUpdate && reInitContentUpdate();
  window.setUISettings && setUISettings();
  window.restartSlider && restartSlider();
  window.refreshPickerUI && refreshPickerUI();
  window.reloadContent && reloadContent();
}

function loadSettingsFromFileData(data) {
  APP_SETTINGS["api_url"] = data?.url_products ?? "";
  APP_SETTINGS["api_url_2"] = data?.url_promo ?? "";
  APP_SETTINGS["login"] = data?.login ?? "";
  APP_SETTINGS["password"] = data?.password ?? "";
  APP_SETTINGS["show_logo"] = data?.show_logo ?? false;
  APP_SETTINGS["update_interval"] = data?.update_interval ?? "";
  APP_SETTINGS["images_padding"] = data?.images_padding ?? "";
  APP_SETTINGS["hide_blue_rectangle"] = data?.hide_blue_rectangle ?? "";
  APP_SETTINGS["slider_interval"] = data?.slider_interval ?? "";
  APP_SETTINGS["slider_speed"] = data?.slider_speed ?? "";
  APP_SETTINGS["border_mode"] = data?.border_mode ?? "";
  APP_SETTINGS["border_color"] = data?.border_color ?? "";
  APP_SETTINGS["border_speed"] = data?.border_speed ?? "";
  APP_SETTINGS["border_intensity"] = data?.border_intensity ?? "";
  APP_SETTINGS["kiosk_mode"] = data?.kiosk_mode ?? "";
  APP_SETTINGS["picker_key"] = data?.picker_key ?? "";
  APP_SETTINGS["show_cards"] = data?.show_cards ?? "";
  applySavedSettingsToForm();
  applySettings();
}

function setting(name) {
  const toNumber = (name) => {
    let result = APP_SETTINGS[name];
    if (typeof result !== "number") {
      result = parseFloat(String(result).replace(",", "."));
    }
    return result;
  };

  switch (name) {
    case "api_url":
      return (
        APP_SETTINGS[name] || "http://188.0.191.18:555/ut_bitrix/hs/infokiosk"
      ).replace(/\/$/, "");

    case "api_url_2":
      return (
        APP_SETTINGS[name] || "http://188.0.191.18:555/ut_bitrix/hs/infokiosk"
      ).replace(/\/$/, "");

    case "login":
      return APP_SETTINGS[name] || "АпГрейд";

    case "password":
      return APP_SETTINGS[name] || "";

    case "update_interval":
      return toNumber(name) || 60 * 60;

    case "show_logo":
      return APP_SETTINGS[name] || false;

    case "images_padding":
      return toNumber(name) || 0;

    case "hide_blue_rectangle":
      return APP_SETTINGS[name] || false;

    case "slider_interval":
      return toNumber(name) || 4;

    case "slider_speed":
      return toNumber(name) || 1;

    // ── Настройки подсветки рамки (F3). Раньше для них не было веток в
    //    setting(), поэтому форма при каждом открытии затирала их пустым
    //    значением, а сохранение сбрасывало выбранный цвет/режим. Теперь
    //    значения корректно читаются, валидируются и имеют дефолты.
    case "border_mode": {
      const v = APP_SETTINGS[name];
      return ["off", "solid", "pulse", "flow", "rainbow"].includes(v) ? v : "rainbow";
    }

    case "border_color": {
      const v = APP_SETTINGS[name];
      return /^#[0-9a-fA-F]{6}$/.test(v) ? v : "#e73a7c";
    }

    case "border_speed": {
      const v = toNumber(name);
      return isNaN(v) || v <= 0 ? 6 : v;
    }

    case "border_intensity": {
      let v = toNumber(name);
      if (isNaN(v)) v = 0.7;
      return Math.min(1, Math.max(0, v));
    }

    // ── Витрина карточек и подборка по цене
    case "kiosk_mode":
      return APP_SETTINGS[name] === "picker" ? "picker" : "showcase";

    case "picker_key":
      return APP_SETTINGS[name] || "F9";

    case "show_cards": {
      const v = APP_SETTINGS[name];
      return !(v === false || v === "false");
    }

    default:
      return "";
  }
}

function applySavedSettingsToForm() {
  for (const input of inputs) {
    if (input.type === "checkbox") {
      input.checked = setting(input.name);
    } else {
      // text / number / color / range / select-one — всем подходит value
      input.value = setting(input.name);
    }
  }
}

function initAuthToken() {
  const credentials = [setting("login"), setting("password")];
  AuthToken = window.btoa(unescape(encodeURIComponent(credentials.join(":"))));
}

function toggleSettingsWindow() {
  if (!SETTINGS_OPENED) {
    settingsEl.removeAttribute("hidden");
  } else {
    settingsEl.setAttribute("hidden", "hidden");
  }

  SETTINGS_OPENED = !SETTINGS_OPENED;
}

/*** INIT ***/

const settingsEl = document.getElementById("settings");

SETTINGS_OPENED = false;

let settings = {};
try {
  const lsSettings = localStorage.getItem("settings");
  settings = JSON.parse(lsSettings);
} catch (e) {}

window.electronAPI.settingsFromFile().then((response) => {
  const { data, settingsFilePath } = response || {};
  console.log("Trying load data from", settingsFilePath);
  data && loadSettingsFromFileData(data);
});

APP_SETTINGS = settings || {};

// Берём и <input>, и <select> — раньше выпадающие списки в форму не
// попадали, поэтому выбор режима подсветки не сохранялся.
const inputs = document.querySelectorAll("#settings input, #settings select");

applySavedSettingsToForm();

let AuthToken;

initAuthToken();

/*** EVENTS ***/

document.addEventListener("keydown", function (event) {
  if (event.code === "F3") {
    event.preventDefault();
    toggleSettingsWindow();
  }
});

document
  .querySelector("#settings .cancel")
  .addEventListener("click", function () {
    toggleSettingsWindow();
  });

document
  .querySelector("#settings form")
  .addEventListener("submit", function (event) {
    event.preventDefault();

    for (const input of inputs) {
      if (input.type === "checkbox") {
        APP_SETTINGS[input.name] = input.checked;
      } else {
        // text / number / color / range / select-one
        APP_SETTINGS[input.name] = input.value;
      }
    }

    toggleSettingsWindow();
    applySettings();
  });

document.querySelector("#settings .export").addEventListener("click", () => {
  window.electronAPI.saveSettings(collectSettings());
});

document
  .querySelector("#settings .import")
  .addEventListener("click", async () => {
    const data = await window.electronAPI.loadSettings();
    if (data === false) return;
    if (data === "error") {
      alert("Ошибка при загрузке файла настроек");
      return;
    }

    loadSettingsFromFileData(data);
  });

document.querySelector("#settings").addEventListener("dblclick", (e) => {
  e.stopPropagation();
});



// «Запомнить кнопку»: следующее нажатие (USB-кнопки) записывается как
// клавиша подборки. Esc — отмена.
(function () {
  const btn = document.querySelector("#settings .learn-key");
  const field = document.querySelector('#settings input[name="picker_key"]');
  if (!btn || !field) return;
  btn.addEventListener("click", function () {
    const before = field.value;
    field.value = "нажмите кнопку…";
    btn.disabled = true;
    function onKey(e) {
      if (e.code === "F3") return;
      e.preventDefault();
      e.stopPropagation();
      field.value = e.code === "Escape" ? before : e.code || e.key;
      btn.disabled = false;
      document.removeEventListener("keydown", onKey, true);
    }
    document.addEventListener("keydown", onKey, true);
  });
})();
