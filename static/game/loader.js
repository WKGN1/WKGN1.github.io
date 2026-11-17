// 方块彩蛋的页面侧：拉 wasm（带进度）、读存档、写存档、更新 HUD、导出导入。
//
// Rust 那边只提供 4 个方向：Rust 调 JS（wasm 导入）+ JS 读 wasm 内存。
// 没有 wasm-bindgen，也没有打包器，浏览器里跑的就是这几个文件本身。
// 语法刻意保守（不用 ?. 和 ??）：微信安卓端的 X5 内核常见还是 Chromium 77。

var DB_NAME = 'mcl-lite';
var DB_VERSION = 1;
var STORE = 'save';
var KEY = 'latest';
var LS_KEY = 'mcl-lite:save';

var STATE_LOADING = 0; // 还在读
var STATE_NONE = 1; // 读完了，没有存档
var STATE_READY = 2; // 读完了，有存档

var saveState = STATE_LOADING;
var saveBytes = null;
var pending = 0; // 已交给存储但没写完的次数
var idbOk = true;
var manifest = null;
var booted = false;
var footOriginal = '';

function $(id) { return document.getElementById(id); }

// ---------------------------------------------------------------------------
// 本地存储：IndexedDB，退路 localStorage
// ---------------------------------------------------------------------------

function hasIdb() {
  try {
    return typeof indexedDB !== 'undefined' && indexedDB !== null;
  } catch (e) {
    return false;
  }
}

function openDb() {
  return new Promise(function (resolve, reject) {
    var req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = function () {
      var db = req.result;
      if (!db.objectStoreNames.contains(STORE)) db.createObjectStore(STORE);
    };
    req.onsuccess = function () { resolve(req.result); };
    req.onerror = function () { reject(req.error || new Error('idb open failed')); };
    req.onblocked = function () { reject(new Error('idb blocked')); };
  });
}

function b64encode(u8) {
  var CH = 8192, parts = [], i;
  for (i = 0; i < u8.length; i += CH) {
    var slice = u8.subarray(i, Math.min(i + CH, u8.length));
    var s = '';
    for (var j = 0; j < slice.length; j++) s += String.fromCharCode(slice[j]);
    parts.push(btoa(s));
  }
  return parts.join('');
}

function b64decode(str) {
  var bin = atob(str), out = new Uint8Array(bin.length);
  for (var i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function lsWrite(u8) {
  try { localStorage.setItem(LS_KEY, b64encode(u8)); return true; } catch (e) { return false; }
}

function lsRead() {
  try {
    var s = localStorage.getItem(LS_KEY);
    return s ? b64decode(s) : null;
  } catch (e) { return null; }
}

function lsClear() {
  try { localStorage.removeItem(LS_KEY); } catch (e) {}
}

function loadSave() {
  if (!hasIdb()) {
    idbOk = false;
    var fb = lsRead();
    if (fb) { saveBytes = fb; saveState = STATE_READY; } else { saveState = STATE_NONE; }
    return;
  }
  openDb()
    .then(function (db) {
      return new Promise(function (resolve, reject) {
        var get = db.transaction(STORE, 'readonly').objectStore(STORE).get(KEY);
        get.onsuccess = function () { resolve(get.result || null); };
        get.onerror = function () { reject(get.error || new Error('idb get failed')); };
      });
    })
    .then(function (found) {
      if (found) {
        saveBytes = found instanceof Uint8Array ? found : new Uint8Array(found);
        saveState = STATE_READY;
      } else {
        var fb = lsRead();
        if (fb) { saveBytes = fb; saveState = STATE_READY; } else { saveState = STATE_NONE; }
      }
    })
    .catch(function () {
      idbOk = false;
      var fb = lsRead();
      if (fb) { saveBytes = fb; saveState = STATE_READY; } else { saveState = STATE_NONE; }
    });
}

// 交给存储。IDB 写失败（配额、正在被清理）退回 localStorage，至少这次不丢。
function putSave(bytes) {
  pending++;
  var done = function (ok) {
    pending--;
    if (ok) {
      saveBytes = bytes;
      if (saveState === STATE_NONE) saveState = STATE_READY;
    }
  };
  if (!idbOk) { done(lsWrite(bytes)); return; }

  openDb()
    .then(function (db) {
      return new Promise(function (resolve, reject) {
        var tx = db.transaction(STORE, 'readwrite');
        tx.objectStore(STORE).put(bytes, KEY);
        tx.oncomplete = function () { resolve(true); };
        tx.onerror = function () { reject(tx.error || new Error('idb write failed')); };
        tx.onabort = function () { reject(tx.error || new Error('idb aborted')); };
      });
    })
    .then(function () { done(true); })
    .catch(function () { done(lsWrite(bytes)); });
}

function clearSave() {
  saveBytes = null;
  saveState = STATE_NONE;
  lsClear();
  if (!idbOk) return Promise.resolve();
  return openDb()
    .then(function (db) {
      return new Promise(function (resolve, reject) {
        var tx = db.transaction(STORE, 'readwrite');
        tx.objectStore(STORE).delete(KEY);
        tx.oncomplete = function () { resolve(true); };
        tx.onerror = function () { reject(tx.error || new Error('idb delete failed')); };
      });
    })
    .catch(function () {});
}

// ---------------------------------------------------------------------------
// wasm 侧的桥：把 4 个 JS 函数挂成 wasm 导入
// ---------------------------------------------------------------------------

// 注意：wasm 内存增长会让旧的 ArrayBuffer 失效（detach），
// 所以每次都要现取 window.wasm_memory.buffer，别缓存。
function wasmView(ptr, len) {
  var mem = window.wasm_memory;
  if (!mem) return new Uint8Array(0);
  return new Uint8Array(mem.buffer, ptr, len);
}

function registerBridge() {
  if (typeof window.miniquad_add_plugin !== 'function') return;
  var decoder = new TextDecoder('utf-8');

  window.miniquad_add_plugin({
    name: 'mcl_bridge',
    version: 1,

    register_plugin: function (importObject) {
      var env = importObject.env;

      // HUD：Rust 每 0.2 秒推一次 "槽位|方块|模式|时刻|改动|帧率|存档|备注"
      env.mcl_hud = function (ptr, len) {
        window.__MCL_HUD = decoder.decode(wasmView(ptr, len));
      };

      // 存档：拷一份出来再写，wasm 那边随时可能复用那块内存
      env.mcl_save_request = function (ptr, len) {
        var src = wasmView(ptr, len);
        putSave(new Uint8Array(src.slice ? src.slice(0) : Array.prototype.slice.call(src)));
      };

      // 世界就绪：Rust 喊一声，这边就把门面收掉、HUD 亮起来（watchBoot 认这个）。
      env.mcl_booted = function () { window.__MCL_BOOTED = true; };

      // Rust 的报错/进度：既进 console（给开发者），也写到底部那一行（给读者）。
      env.mcl_log = function (ptr, len) {
        var m = decoder.decode(wasmView(ptr, len));
        window.__MCL_LOGS = (window.__MCL_LOGS || []).concat(m);
        console.warn('[彩蛋] ' + m);
        // 启动阶段的话读者不用看，进 console 就够；崩了这种话必须让他看见。
        if (m.indexOf('崩') >= 0 || m.indexOf('跑不了') >= 0) { try { say(m); } catch (e) {} }
      };

      env.mcl_save_pending = function () { return pending; };
      env.mcl_save_ready = function () { return saveState; };

      // 把存档字节写进 Rust 给的缓冲区，返回真实长度
      env.mcl_pull_save = function (ptr, cap) {
        if (!saveBytes || !window.wasm_memory) return 0;
        var n = Math.min(saveBytes.length, cap);
        wasmView(ptr, n).set(saveBytes.subarray(0, n));
        return n;
      };
    },

    on_init: function () {}
  });
}

// ---------------------------------------------------------------------------
// 加载 wasm（自己 fetch 是为了能显示进度，然后交给胶水脚本的 load()）
// ---------------------------------------------------------------------------

function fmtSize(n) {
  if (n >= 1024 * 1024) return (n / (1024 * 1024)).toFixed(2) + ' MB';
  return Math.round(n / 1024) + ' KB';
}

function fetchWithProgress(url, totalHint) {
  return fetch(url).then(function (res) {
    if (!res.ok) throw new Error('HTTP ' + res.status);
    var total = parseInt(res.headers.get('Content-Length') || '', 10) || totalHint || 0;
    var elBytes = $('load-bytes');
    var elBar = $('bar-fill');

    if (!res.body || !res.body.getReader) {
      // 老内核没有流式 body：不报中间进度，直接整块读
      return res.arrayBuffer().then(function (buf) {
        if (elBytes) elBytes.textContent = fmtSize(buf.byteLength) + ' 已下载';
        if (elBar) elBar.style.width = '100%';
        return new Blob([buf], { type: 'application/wasm' });
      });
    }

    var reader = res.body.getReader();
    var chunks = [];
    var got = 0;
    return pump();

    function pump() {
      return reader.read().then(function (r) {
        if (r.done) {
          if (elBytes) elBytes.textContent = fmtSize(got) + ' 已下载';
          if (elBar) elBar.style.width = '100%';
          return new Blob(chunks, { type: 'application/wasm' });
        }
        chunks.push(r.value);
        got += r.value.length;
        if (elBytes) elBytes.textContent = fmtSize(got) + ' / ' + fmtSize(total || got);
        if (elBar && total) elBar.style.width = Math.min(100, (got / total) * 100).toFixed(1) + '%';
        return pump();
      });
    }
  });
}

function startGame() {
  var btn = $('btn-play');
  if (btn) { btn.dataset.loaded = '1'; btn.disabled = true; btn.textContent = '加载中'; }
  var load = $('load');
  if (load) load.hidden = false;
  var elState = $('load-state');

  // 注意：wasm 的名字带内容哈希，只有 manifest.json 读回来之后才知道叫什么，
  // 所以这个 url 必须等 manifest 到手之后再定（之前定在函数开头，拿到的永远是
  // 兜底的 mc-lite.wasm，于是 404 —— 实测过：资源表里就写着 mc-lite.wasm 404）。
  fetch('manifest.json').then(function (r) {
    return r.ok ? r.json() : null;
  }).catch(function () {
    return null;
  }).then(function (m) {
    manifest = m || null;
    var totalHint = manifest && manifest.bytes ? manifest.bytes : 0;
    var url = manifest && manifest.wasm ? manifest.wasm : 'mc-lite.wasm';
    if (elState) elState.textContent = '下载 wasm（' + fmtSize(totalHint) + '）';

    return fetchWithProgress(url, totalHint);
  }).then(function (blob) {
    if (elState) elState.textContent = '编译 wasm，这台机器慢的话要几秒';
    // 已经下好的字节用 blob URL 交给胶水，load() 里的 fetch 就当本地文件，秒完。
    window.load(URL.createObjectURL(blob));
    watchBoot();
  }).catch(function (err) {
    showFallback('加载失败：' + (err && err.message ? err.message : err));
  });
}

// ---------------------------------------------------------------------------
// HUD 与门面
// ---------------------------------------------------------------------------

function showFallback(msg) {
  var f = $('fallback');
  var btn = $('btn-play');
  if (f) {
    f.hidden = false;
    if (msg) f.textContent = msg;
  }
  if (btn) btn.style.display = 'none';
}

var HUD_IDS = ['hud-slot', 'hud-block', 'hud-mode', 'hud-clock', 'hud-changed', 'hud-fps', 'hud-save'];

function tickHud() {
  var raw = window.__MCL_HUD;
  if (!raw) return;
  var parts = String(raw).split('|');
  for (var i = 0; i < HUD_IDS.length; i++) {
    var el = $(HUD_IDS[i]);
    if (el && parts[i] != null) el.textContent = parts[i];
  }
}

function watchBoot() {
  var timer = setInterval(function () {
    if (!window.__MCL_BOOTED) return;
    clearInterval(timer);
    booted = true;

    var gate = $('gate');
    if (gate) gate.hidden = true;
    // 起得慢的机器（ wasm 编译要几秒）会被那个 2.5 秒的兜底先抢跑一步，
    // 真跑起来之后必须把它收掉，否则页面上一直挂着「这台机器的浏览器跑不了」。
    var fb = $('fallback');
    if (fb) fb.hidden = true;
    var hud = $('hud');
    if (hud) hud.hidden = false;
    var cross = $('cross');
    if (cross) cross.hidden = false;

    ['btn-export', 'btn-import', 'btn-reset'].forEach(function (id) {
      var b = $(id);
      if (b) b.disabled = false;
    });

    // 触屏设备换一套操作提示
    var touchish = ('ontouchstart' in window) || (navigator.maxTouchPoints > 0);
    var hd = $('hint-desktop');
    var ht = $('hint-touch');
    if (touchish) {
      if (hd) hd.hidden = true;
      if (ht) ht.hidden = false;
    }

    setInterval(tickHud, 200);
  }, 200);
}

function say(msg) {
  var el = $('foot-note');
  if (!el) return;
  if (!footOriginal) footOriginal = el.textContent;
  el.textContent = msg;
  clearTimeout(say._t);
  say._t = setTimeout(function () { el.textContent = footOriginal; }, 4000);
}

function stamp() {
  var d = new Date();
  function p(n) { return (n < 10 ? '0' : '') + n; }
  return '' + d.getFullYear() + p(d.getMonth() + 1) + p(d.getDate()) + '-' + p(d.getHours()) + p(d.getMinutes());
}

function wireButtons() {
  var play = $('btn-play');
  if (play) play.addEventListener('click', function () {
    if (play.dataset.loaded) return;
    startGame();
  });

  var ex = $('btn-export');
  if (ex) ex.addEventListener('click', function () {
    if (!saveBytes) { say('还没有存档：先在岛上挖两下，等 HUD 上显示「已存档」。'); return; }
    var url = URL.createObjectURL(new Blob([saveBytes], { type: 'application/octet-stream' }));
    var a = document.createElement('a');
    a.href = url;
    a.download = '方块存档-' + stamp() + '.mcl';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setTimeout(function () { URL.revokeObjectURL(url); }, 2000);
    say('已导出 ' + fmtSize(saveBytes.length) + ' 的文件，发给别人或换电脑时用「导入」。');
  });

  var im = $('btn-import');
  var file = $('file-import');
  if (im && file) im.addEventListener('click', function () { file.click(); });
  if (file) file.addEventListener('change', function () {
    var f = file.files && file.files[0];
    if (!f) return;
    var rd = new FileReader();
    rd.onload = function () {
      var bytes = new Uint8Array(rd.result);
      saveBytes = bytes;
      saveState = STATE_READY;
      putSave(bytes);
      // 世界在 wasm 里是常驻内存的，换存档最省事的办法是整页重来
      setTimeout(function () { location.reload(); }, 120);
      say('正在用导入的文件重建世界…');
    };
    rd.onerror = function () { say('这个文件读不出来。'); };
    rd.readAsArrayBuffer(f);
  });

  var rs = $('btn-reset');
  if (rs) rs.addEventListener('click', function () {
    clearSave();
    setTimeout(function () { location.reload(); }, 120);
    say('已清掉本地存档，正在按种子重生成一座新岛…');
  });
}

// ---------------------------------------------------------------------------
// 起手
// ---------------------------------------------------------------------------

registerBridge();
loadSave();
wireButtons();

if (typeof window.load !== 'function') {
  showFallback('胶水脚本 mq_js_bundle.js 没加载上，这个彩蛋跑不起来。');
}

// 无头浏览器截图 / 自动化测试用：地址后加 ?autoplay=1 就直接开跑，不用人去点按钮。
if (typeof window.load === 'function' && /[?&]autoplay=1/.test(location.search)) startGame();
