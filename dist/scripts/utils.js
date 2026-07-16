function fetchFromServer(path, cb, reject) {
  const xhr = new XMLHttpRequest();
  xhr.withCredentials = true;

  xhr.addEventListener("readystatechange", function () {
    if (this.readyState === 4) {
      try {
        cb(JSON.parse(this.responseText));
      } catch (e) {
        reject(e.message);
      }
    }
  });

  xhr.open("GET", path);
  xhr.setRequestHeader("Authorization", "Basic " + AuthToken);

  xhr.send();
}
