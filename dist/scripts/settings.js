/*** UTILS ***/

function applySettings() {
  localStorage.setItem("settings", JSON.stringify(APP_SETTINGS));

  initAuthToken();
  window.reInitContentUpdate && reInitContentUpdate();
  window.setUISettings && setUISettings();
  window.restartSlider && restartSlider();
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

    default:
      return "";
  }
}

function applySavedSettingsToForm() {
  for (const input of inputs) {
    if (input.type === "text") {
      input.value = setting(input.name);
    } else if (input.type === "checkbox") {
      input.checked = setting(input.name);
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

const inputs = document.querySelectorAll("#settings input");

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
      const type = input.getAttribute("type");
      if (type === "text") {
        APP_SETTINGS[input.name] = input.value;
      } else if (type === "checkbox") {
        APP_SETTINGS[input.name] = input.checked;
      }
    }

    toggleSettingsWindow();
    applySettings();
  });

document.querySelector("#settings .export").addEventListener("click", () => {
  window.electronAPI.saveSettings({
    url_products: setting("api_url"),
    url_promo: setting("api_url_2"),
    login: setting("login"),
    password: setting("password"),
    // update_interval: setting("update_interval"),
    show_logo: setting("show_logo"),
    // images_padding: setting("images_padding"),
    // hide_blue_rectangle: setting("hide_blue_rectangle"),
    slider_interval: setting("slider_interval"),
    // slider_speed: setting("slider_speed"),
  });
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
