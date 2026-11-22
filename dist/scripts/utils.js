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

// Длинные названия из 1С («Инфракрасный звуковой датчик Беспроводной На
// батарейках Расстояние движения: 2-8м Цвет: Белый») делим на главное —
// что это за товар — и подробности, которые показываются мельче.
// Граница: после 16+ символов перед словом с заглавной буквы или цифры,
// после запятой/двоеточия; иначе — по последнему пробелу до ~34 символов.
window.kioskSplitTitle = function (name, characteristic) {
  name = String(name || "").replace(/\s+/g, " ").trim();
  characteristic = String(characteristic || "").replace(/\s+/g, " ").trim();
  let main = name;
  let rest = "";
  if (name.length > 34) {
    const words = name.split(" ");
    let acc = "";
    let cut = -1;
    let fallback = -1;
    for (let i = 0; i < words.length - 1; i++) {
      acc = acc ? acc + " " + words[i] : words[i];
      const next = words[i + 1];
      if (acc.length <= 34) fallback = i;
      if (acc.length >= 16 && acc.length <= 44) {
        if (/[,:;]$/.test(words[i]) || /^[A-ZА-ЯЁ0-9«"(]/.test(next)) {
          cut = i;
          break;
        }
      }
      if (acc.length > 44) break;
    }
    if (cut < 0) cut = fallback;
    if (cut >= 0) {
      main = words.slice(0, cut + 1).join(" ").replace(/[,:;]$/, "");
      rest = words.slice(cut + 1).join(" ");
    }
  }
  if (characteristic) rest = rest ? rest + ", " + characteristic : characteristic;
  return { main: main, rest: rest };
};
