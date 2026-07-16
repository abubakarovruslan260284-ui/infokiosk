const codeEl = document.getElementById("code");
const waitingEl = document.getElementById("waiting");
const infoEl = document.getElementById("info");

function switchToWaiting() {
  setTimeout(function () {
    waitingEl.removeAttribute("hidden");
    infoEl.setAttribute("hidden", "hidden");
  }, 600);
  setTimeout(function () {
    waitingEl.classList.add("animate");
  }, 700);
  infoEl.classList.remove("animate");
}

function switchToInfo() {
  waitingEl.setAttribute("hidden", "hidden");
  infoEl.removeAttribute("hidden");

  waitingEl.classList.remove("animate");

  setTimeout(function () {
    infoEl.classList.add("animate");
  }, 100);
}

function formatCents(val) {
  const normalized = parseInt(val * 100);
  if (normalized < 10) return "0" + normalized;
  return normalized;
}

function setData(data) {
  // document.querySelector('#info .sku').textContent = data.artikul;
  document.querySelector("#info .title").innerHTML =
    data.name + "<br>" + data.characteristic;
  document.querySelector("#info .price .value").textContent = parseInt(
    data.price
  );
  document.querySelector("#info .price .value-after").textContent = formatCents(
    data.price - parseInt(data.price)
  );
  document.querySelector("#info .discounted-price .value").textContent =
    parseInt(data.discounted_price);
  document.querySelector("#info .discounted-price .value-after").textContent =
    formatCents(data.discounted_price - parseInt(data.discounted_price));
  document.querySelector("#info .discount-percent").textContent =
    -data.discount_percent + "%";
}

infoEl.setAttribute("hidden", "hidden");
waitingEl.classList.add("animate");
codeEl.focus();

setInterval(function () {
  if (!SETTINGS_OPENED) {
    codeEl.focus();
  }
}, 1000);

function showInfo(data) {
  setData(data);
  switchToInfo();

  clearTimeout(timeout);
  timeout = setTimeout(function () {
    switchToWaiting();
  }, 14000);
}

let timeout;
codeEl.addEventListener("change", function () {
  fetchFromServer(setting("api_url") + "/getgood/" + codeEl.value, showInfo);
  codeEl.value = "";
});

// fetchData('00000011517184', showInfo)

window.onerror = function () {
  if (codeEl.style.opacity !== 1) {
    // window.location.reload();
  }
};

document.addEventListener("keydown", function (event) {
  if (event.code === "F2") {
    event.preventDefault();
    codeEl.style.opacity = 1 - codeEl.style.opacity;
  }

  if (event.code === "F7") {
    showInfo({
      code: "СК-00022825",
      name: '"BUBBLE TIME" Краситель флуоресцентный PIM/F-15 15 мл 05 Оранжевый',
      artikul: "",
      barcode: "4680269373655",
      discount_percent: "5",
      price: "254",
      discounted_price: "241.10",
      characteristic: "",
    });

    clearTimeout(timeout);
  }
});

document.addEventListener("dblclick", () => {
  window.electronAPI.exitFullscreen();
});
