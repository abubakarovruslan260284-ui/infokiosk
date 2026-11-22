/**
 * «Карточка товара» — страница для телефона сотрудника.
 *
 * Google Apps Script внутри аккаунта irskom.pr. Страница открывается по
 * ссылке на любом телефоне: сотрудник щёлкает кольцом-сканером по
 * штрихкоду, снимает короткий ролик товара и фото обложки — файлы
 * сохраняются в папку Google Диска «Карточки Товаров» с правильными
 * именами. Дальше их забирает служба карточек на PC-38.
 *
 * Развёртывание: Начать развёртывание → Веб-приложение,
 *   «Выполнять как: Я (irskom.pr)», «Доступ: Все» (или «Все, у кого
 *   есть аккаунт Google»). Ссылку на /exec открыть на айфоне и
 *   добавить на экран «Домой».
 */

// Папка «Карточки Товаров» на Google Диске irskom.pr.
var FOLDER_ID = '1M70ZGVnOIYdx57FPzt-oSg4K0VST6rdO';

// Простая защита от посторонних: ссылка + этот код в адресе (?k=...).
// Пусто — без кода.
var ACCESS_KEY = '';

function doGet(e) {
  if (ACCESS_KEY && (!e || !e.parameter || e.parameter.k !== ACCESS_KEY)) {
    return HtmlService.createHtmlOutput('<p style="font:20px sans-serif;padding:24px">Нет доступа</p>');
  }
  var t = HtmlService.createTemplateFromFile('index');
  t.accessKey = ACCESS_KEY ? e.parameter.k : '';
  return t
    .evaluate()
    .setTitle('Карточка товара')
    .addMetaTag('viewport', 'width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no')
    .setXFrameOptionsMode(HtmlService.XFrameOptionsMode.DEFAULT);
}

/**
 * Сохраняет один файл (ролик или фото обложки) в папку.
 * kind: 'video' | 'cover'. Возвращает имя созданного файла.
 */
function saveItem(key, code, stamp, kind, ext, mime, base64) {
  if (ACCESS_KEY && key !== ACCESS_KEY) throw new Error('Нет доступа');
  code = String(code || '').replace(/[^0-9A-Za-z-]/g, '');
  stamp = String(stamp || '').replace(/[^0-9-]/g, '');
  if (!code || code.length > 40) throw new Error('Пустой или неверный штрихкод');
  if (!/^\d{8}-\d{6}$/.test(stamp)) throw new Error('Неверное время');
  if (kind !== 'video' && kind !== 'cover' && kind !== 'label' && kind !== 'photo') throw new Error('Неверный вид файла');
  ext = String(ext || '').toLowerCase().replace(/[^a-z0-9]/g, '');
  var allowed = { jpg: 1, jpeg: 1, mp4: 1, mov: 1, m4v: 1 };
  if (!allowed[ext]) throw new Error('Неподдерживаемый формат: ' + ext);

  var name = code + '__' + stamp + '__' + kind + '.' + ext;
  var bytes = Utilities.base64Decode(base64);
  if (bytes.length > 45 * 1024 * 1024) throw new Error('Файл слишком большой — снимайте ролик 2–3 секунды');
  var blob = Utilities.newBlob(bytes, mime || 'application/octet-stream', name);
  var folder = DriveApp.getFolderById(FOLDER_ID);
  // Сначала под временным именем, потом переименовываем: служба не
  // увидит файл, пока он не записан целиком.
  var file = folder.createFile(blob.setName('.' + name + '.part'));
  file.setName(name);
  return name;
}

/** Сколько файлов ждут обработки и сколько ушло в ошибки — для подсказки на странице. */
function status(key) {
  if (ACCESS_KEY && key !== ACCESS_KEY) throw new Error('Нет доступа');
  var folder = DriveApp.getFolderById(FOLDER_ID);
  var waiting = 0;
  var it = folder.getFiles();
  while (it.hasNext()) {
    var n = it.next().getName();
    if (/__(video|cover|label|photo)\.[a-z0-9]+$/i.test(n)) waiting++;
  }
  var errors = 0;
  var sub = folder.getFoldersByName('_Ошибки');
  if (sub.hasNext()) {
    var ei = sub.next().getFiles();
    while (ei.hasNext()) {
      if (!/\.csv$/i.test(ei.next().getName())) errors++;
    }
  }
  return { waiting: waiting, errors: errors };
}
