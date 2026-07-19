// Запрос цены идёт через Rust-команду fetch_price (см. src-tauri/src/main.rs),
// а не через XMLHttpRequest из браузера — WebView2 строго соблюдает CORS,
// а сервис 1С не отвечает нужными заголовками для межсайтовых запросов
// из браузера. У серверных запросов из Rust концепции CORS нет вообще.
// Сигнатура функции и её использование в script.js НЕ меняются.
function fetchFromServer(path, cb, reject) {
  reject = reject || function (msg) { console.warn("fetchFromServer: ошибка —", msg); };
  const invoke = window.__TAURI__.core.invoke;
  invoke("fetch_price", { url: path, authToken: AuthToken })
    .then(function (data) { cb(data); })
    .catch(function (e) {
      reject(typeof e === "string" ? e : (e && e.message) || String(e));
    });
}
