const sliderContainerEl = document.querySelector("#slider ul");

const ENTER_CLASS_NAME = "enter";
const EXIT_CLASS_NAME = "exit";

function sliderStart() {
  const slides = sliderContainerEl.querySelectorAll("li");
  slides.forEach((slide) => {
    slide.classList.remove(ENTER_CLASS_NAME);
    slide.classList.remove(EXIT_CLASS_NAME);
  });

  const currentEnter = sliderContainerEl.querySelector("li");
  if (currentEnter) {
    currentEnter.classList.add(ENTER_CLASS_NAME);
  }
}

function slideNext() {
  const currentExit = sliderContainerEl.querySelector("." + EXIT_CLASS_NAME);
  let currentEnter = sliderContainerEl.querySelector("." + ENTER_CLASS_NAME);
  if (!currentEnter) currentEnter = sliderContainerEl.querySelector("li");

  if (currentExit) currentExit.classList.remove(EXIT_CLASS_NAME);

  if (currentEnter) {
    currentEnter.classList.remove(ENTER_CLASS_NAME);
    currentEnter.classList.add(EXIT_CLASS_NAME);
  }

  let next = currentEnter && currentEnter.nextElementSibling;
  if (!next) next = sliderContainerEl.querySelector("li");
  if (next) {
    next.classList.add(ENTER_CLASS_NAME);
    next.classList.remove(EXIT_CLASS_NAME);
  }
}

let sliderInterval = null;
function restartSlider() {
  sliderInterval !== null && clearInterval(sliderInterval);

  sliderStart();

  const timeout = setting("slider_interval") + setting("slider_speed");
  sliderInterval = setInterval(slideNext, timeout * 1000);

  const sliderEl = document.querySelector("#slider");
  const sliderSpeed = setting("slider_speed");
  sliderEl.style.setProperty("--slider-speed", sliderSpeed + "s");
}

restartSlider();
