// 方块彩蛋的页面侧：拉 wasm（带进度）、读存档、写存档、更新 HUD、导出导入。
//
// Rust 那边只提供几个方向：Rust 调 JS（wasm 导入）+ JS 读 wasm 内存。
// 没有 wasm-bindgen，也没有打包器，浏览器里跑的就是这几个文件本身。
// 语法刻意保守（不用箭头函数、const/let、模板字符串、可选链、class）：
// 微信安卓端的 X5 内核常见还是 Chromium 77，iOS 那边还要照顾 12 及更早的 WKWebView。
// Promise / fetch / Symbol / Object.defineProperty 是可以用的。

// 门面那段内联脚本靠这个变量判断「模块脚本到底执行了没有」，所以它必须是本文件最前面
// 一句真正执行的代码（放在注释后面、任何可能出错的调用之前）。
window.__MCL_LOADER_RAN = 1;

// ---------------------------------------------------------------------------
// 老内核补丁：TouchList 不可迭代
//
// 胶水脚本每个触摸事件都是 `for (const t of e.changedTouches)`。iOS 12 及更早、
// 还有一批老 X5 里 TouchList 没有 [Symbol.iterator]，于是每个 touchstart 直接抛
// TypeError：手机上碰屏幕完全没反应，还会被内联脚本判成「这浏览器跑不了」。
// 这里赶在任何 wasm 调用之前补一个纯 ES5 的迭代器（item 6）。
// ---------------------------------------------------------------------------
(function patchTouchList() {
  try {
    if (typeof Symbol === 'undefined' || !Symbol.iterator) return;
    var proto = window.TouchList && window.TouchList.prototype;
    if (!proto || typeof proto.item !== 'function') return;
    if (typeof proto[Symbol.iterator] === 'function') return; // 新内核本来就能 for..of
    Object.defineProperty(proto, Symbol.iterator, {
      configurable: true,
      writable: true,
      value: function () {
        var list = this, i = 0;
        return {
          next: function () {
            var done = i >= list.length;
            var value = done ? undefined : list.item(i);
            i++;
            return { done: done, value: value };
          }
        };
      }
    });
  } catch (e) {}
})();

var DB_NAME = 'mcl-lite';
var DB_VERSION = 1;
var STORE = 'save';
var KEY = 'latest';
var LS_KEY = 'mcl-lite:save';
var NOTE_KEY = 'mcl-lite:note'; // 刷新之前想留下的话（手机上只能靠页脚那一行问人）

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

var OPEN_TIMEOUT_MS = 4000; // indexedDB.open 挂住的兜底
var LOAD_FALLBACK_MS = 3500; // 读存档迟迟不落地就当没有
var BOOT_WATCHDOG_MS = 25000; // wasm 编译/实例化/启动的总时限
var RELOAD_FALLBACK_MS = 2500; // 清档/导入等不到存储写完时的兜底
var IMPORT_MAX_BYTES = 8 * 1024 * 1024;

// 清档/导入之后、刷新之前，别再让还活着的 wasm 往里写，否则它会把刚清掉/刚导入的档盖回去。
var suppressSave = false;
var reloadDone = false;

// DOM 按钮按下来的待取标志（Rust 每帧 take 一次，取走即清）
var wantJump = false;
var wantFly = false;
var wantSlot = 0;

// ---------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------

function $(id) { return document.getElementById(id); }

function retryHref() {
  // 「刷新重试」原来是裸 reload：GitHub Pages 对这些文件发 cache-control: max-age=600，
  // 坏掉的 wasm/manifest 读回来还是同一份缓存，重试几次都是同一个错。
  // 加个时间戳参数让它变成另一个地址。autoplay 的判断照旧成立（正则看 ? 或 & 后面）。
  var q = location.search || '';
  var h = location.hash || '';
  return location.pathname + q + (q ? '&' : '?') + 'retry=' + Date.now() + h;
}

function say(msg) {
  var el = $('foot-note');
  if (!el) return;
  if (!footOriginal) footOriginal = el.textContent;
  el.textContent = msg;
  clearTimeout(say._t);
  say._t = setTimeout(function () { el.textContent = footOriginal; }, 4000);
}

function noteLater(msg) {
  try { localStorage.setItem(NOTE_KEY, String(msg)); } catch (e) {}
}

// 刷新之前留下的话，下次进页脚先念一遍（报错原因不该被刷新吃掉）
function drainNote() {
  var m = '';
  try {
    m = localStorage.getItem(NOTE_KEY) || '';
    if (m) localStorage.removeItem(NOTE_KEY);
  } catch (e) { return; }
  if (!m) return;
  var show = function () { say(m); };
  if (document.readyState === 'complete' || document.readyState === 'interactive') show();
  else if (window.addEventListener) window.addEventListener('DOMContentLoaded', show, false);
  else show();
}

function fmtSize(n) {
  if (n >= 1024 * 1024) return (n / (1024 * 1024)).toFixed(2) + ' MB';
  return Math.round(n / 1024) + ' KB';
}

function stamp() {
  var d = new Date();
  function p(n) { return (n < 10 ? '0' : '') + n; }
  return '' + d.getFullYear() + p(d.getMonth() + 1) + p(d.getDate()) + '-' + p(d.getHours()) + p(d.getMinutes());
}

function describeErr(e) {
  if (!e) return '未知错误';
  if (e.message) return String(e.message);
  if (e.toString) {
    try { return String(e.toString()); } catch (x) {}
  }
  return String(e);
}

// ---------------------------------------------------------------------------
// 本地存储：IndexedDB（连接复用），退路 localStorage
// ---------------------------------------------------------------------------

function hasIdb() {
  try {
    return typeof indexedDB !== 'undefined' && indexedDB !== null;
  } catch (e) {
    return false;
  }
}

var dbConn = null; // 复用同一条连接：每 3 秒自动存档都开一条，玩 10 分钟能攒两百条
var dbOpening = null;

function dropDb() {
  dbOpening = null;
  dbConn = null;
}

function openDb() {
  if (dbConn) return Promise.resolve(dbConn);
  if (dbOpening) return dbOpening;

  var prom = null;
  var settled = false;
  var timer = 0;
  var req = null;
  var rejectFn = null;

  // 注意：reject 只在 executor 里拿得到，所以外面只认 rejectFn
  function settleFail(msg) {
    if (settled) return;
    settled = true;
    if (timer) { clearTimeout(timer); timer = 0; }
    if (dbOpening === prom) dbOpening = null;
    if (rejectFn) { try { rejectFn(new Error(msg)); } catch (e) {} }
  }

  prom = new Promise(function (resolve, reject) {
    rejectFn = reject;
    function fail(msg) { settleFail(msg); }

    // open 没有超时的话，saveState 永远是 0、pending 永远 >0，Rust 卡在等存档，
    // 画布静止也不报错（实测过）。超时就走现成的 localStorage 兜底链。
    // 超时之后真开开了的连接会被 onsuccess 里那条 settled 判断直接 close 掉，不漏。
    timer = setTimeout(function () {
      timer = 0;
      fail('IndexedDB 打开超时（' + (OPEN_TIMEOUT_MS / 1000) + ' 秒）');
    }, OPEN_TIMEOUT_MS);

    try {
      req = indexedDB.open(DB_NAME, DB_VERSION);
    } catch (e) {
      fail('IndexedDB 打不开：' + describeErr(e));
      return;
    }
    req.onupgradeneeded = function () {
      var db = req.result;
      if (!db.objectStoreNames.contains(STORE)) db.createObjectStore(STORE);
    };
    req.onsuccess = function () {
      if (timer) { clearTimeout(timer); timer = 0; }
      if (dbOpening === prom) dbOpening = null;
      var db = req.result;
      if (settled) { // 超时之后才姗姗来迟：这条连接没人要，立刻还回去
        try { db.close(); } catch (e) {}
        return;
      }
      settled = true;
      dbConn = db;
      // 被别的页签抢走 / 被系统回收：缓存作废，下次重新开，别抱着死连接不放
      db.onversionchange = function () {
        try { db.close(); } catch (e) {}
        if (dbConn === db) dropDb();
      };
      db.onclose = function () { if (dbConn === db) dropDb(); };
      resolve(db);
    };
    req.onerror = function () { fail(req.error ? describeErr(req.error) : 'IndexedDB 打开失败'); };
    req.onblocked = function () { fail('IndexedDB 被别的页签占着（onblocked）'); };
  });

  dbOpening = prom;
  prom.then(function () {}, function () {}); // 没人接的 reject 别冒成 unhandledrejection
  return prom;
}

// 一个读写事务跑完才算写完（oncomplete，不是 put 返回）
function runWrite(db, mutate) {
  return new Promise(function (resolve, reject) {
    var tx;
    try { tx = db.transaction(STORE, 'readwrite'); } catch (e) { reject(e); return; }
    try { mutate(tx.objectStore(STORE)); } catch (e) { try { tx.abort(); } catch (x) {} reject(e); return; }
    tx.oncomplete = function () { resolve(true); };
    tx.onerror = function () { reject(tx.error || new Error('IndexedDB 写入失败')); };
    tx.onabort = function () { reject(tx.error || new Error('IndexedDB 写入被中止')); };
  });
}

function readStore(db) {
  return new Promise(function (resolve, reject) {
    var get;
    try { get = db.transaction(STORE, 'readonly').objectStore(STORE).get(KEY); } catch (e) { reject(e); return; }
    get.onsuccess = function () { resolve(get.result || null); };
    get.onerror = function () { reject(get.error || new Error('IndexedDB 读取失败')); };
  });
}

function idbWrite(bytes) {
  return openDb().then(function (db) { return runWrite(db, function (store) { store.put(bytes, KEY); }); });
}

function idbDelete() {
  return openDb().then(function (db) { return runWrite(db, function (store) { store.delete(KEY); }); });
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

// Rust 靠 save_ready 决定要不要等；等它真开口问之后才开始算「等太久」，
// 这样「用户在门面上磨蹭了半分钟才点开玩」不会被误判成读档超时。
var loadFallbackArmed = false;
function armLoadFallback() {
  if (loadFallbackArmed || saveState !== STATE_LOADING) return;
  loadFallbackArmed = true;
  setTimeout(function () {
    if (saveState !== STATE_LOADING) return;
    // 还没落地就当没有存档：否则 Rust 一直等、画布静止、也不报错
    saveState = STATE_NONE;
  }, LOAD_FALLBACK_MS);
}

function loadSave() {
  if (!hasIdb()) {
    idbOk = false;
    var fb = lsRead();
    if (fb) { saveBytes = fb; saveState = STATE_READY; } else { saveState = STATE_NONE; }
    return;
  }
  openDb()
    .then(function (db) { return readStore(db); })
    .then(function (found) {
      if (found) {
        saveBytes = found instanceof Uint8Array ? found : new Uint8Array(found);
        if (saveState === STATE_LOADING) saveState = STATE_READY;
      } else {
        var fb = lsRead();
        if (fb) { saveBytes = fb; if (saveState === STATE_LOADING) saveState = STATE_READY; }
        else if (saveState === STATE_LOADING) saveState = STATE_NONE;
      }
    })
    .catch(function () {
      idbOk = false;
      var fb = lsRead();
      if (fb) { saveBytes = fb; if (saveState === STATE_LOADING) saveState = STATE_READY; }
      else if (saveState === STATE_LOADING) saveState = STATE_NONE;
    });
}

// 交给存储。返回一个 promise，resolve(true)=真写完了 / false=退回 localStorage 也没写成。
// IDB 写失败（配额、正在被清理）退回 localStorage，至少这次不丢。
function putSave(bytes) {
  if (suppressSave) return Promise.resolve(false); // 清档/导入之后、刷新之前不再写
  pending++;
  var settled = false;

  var prom = new Promise(function (resolve) {
    function record(ok) {
      if (!ok) return;
      saveBytes = bytes;
      if (saveState === STATE_NONE) saveState = STATE_READY;
    }
    function settle(ok) {
      if (settled) { record(ok); return; } // 迟到的成功照样记账，只是不再改结论/pending
      settled = true;
      pending--;
      record(ok);
      resolve(!!ok);
    }
    if (!idbOk) { settle(lsWrite(bytes)); return; }

    openDb()
      .then(function (db) { return runWrite(db, function (store) { store.put(bytes, KEY); }); })
      .then(function () { settle(true); }, function () { settle(lsWrite(bytes)); });

    // 存储挂住时 pending 会一直 >0，Rust 会一直显示「保存中」；到点让它继续。
    setTimeout(function () { settle(false); }, 4000);
  });

  prom.then(function () {}, function () {});
  return prom;
}

// 清掉本地存档。返回 promise：存储那边真的删完了才 resolve(true)。
// 这里不设内部超时：调用方（清档按钮）自己有兜底，别提前下一句「没清干净」的判断。
function clearSave() {
  saveBytes = null;
  saveState = STATE_NONE;
  lsClear();
  if (!idbOk) return Promise.resolve(true);

  var settled = false;
  var prom = new Promise(function (resolve) {
    function settle(ok) {
      if (settled) return;
      settled = true;
      resolve(!!ok);
    }
    idbDelete().then(function () { settle(true); }, function () { settle(false); });
  });

  prom.then(function () {}, function () {});
  return prom;
}

// ---------------------------------------------------------------------------
// wasm 侧的桥：把 JS 函数挂成 wasm 导入
// ---------------------------------------------------------------------------

// 注意：wasm 内存增长会让旧的 ArrayBuffer 失效（detach），
// 所以每次都要现取 window.wasm_memory.buffer，别缓存。
function wasmView(ptr, len) {
  var mem = window.wasm_memory;
  if (!mem || !mem.buffer) return new Uint8Array(0);
  if (len <= 0) return new Uint8Array(0);
  try {
    return new Uint8Array(mem.buffer, ptr, len);
  } catch (e) {
    // 指针越界（多半是内存刚增长过）：给个空视图，比抛出去强
    return new Uint8Array(0);
  }
}

var hudDecoder = null;
function getDecoder() {
  if (!hudDecoder) {
    try { hudDecoder = new TextDecoder('utf-8'); } catch (e) { hudDecoder = null; }
  }
  return hudDecoder;
}

function decodeBytes(ptr, len) {
  var dec = getDecoder();
  var view = wasmView(ptr, len);
  if (dec) {
    try { return dec.decode(view); } catch (e) {}
  }
  // 极老的没有 TextDecoder：退回逐段解码，别让整个 wasm 调用抛出去
  var parts = [], i;
  for (i = 0; i < view.length; i += 1024) {
    parts.push(String.fromCharCode.apply(null, Array.prototype.slice.call(view.subarray(i, i + 1024))));
  }
  try { return decodeURIComponent(escape(parts.join(''))); } catch (e2) { return parts.join(''); }
}

function registerBridge() {
  if (typeof window.miniquad_add_plugin !== 'function') return;

  window.miniquad_add_plugin({
    name: 'mcl_bridge',
    version: 1,

    register_plugin: function (importObject) {
      var env = importObject.env;

      // HUD：Rust 每 0.2 秒推一次 "槽位|方块|模式|时刻|改动|帧率|存档|备注"
      env.mcl_hud = function (ptr, len) {
        window.__MCL_HUD = decodeBytes(ptr, len);
      };

      // 存档：拷一份出来再写，wasm 那边随时可能复用那块内存
      env.mcl_save_request = function (ptr, len) {
        if (suppressSave) return; // 清档/导入之后、刷新之前：丢掉，pending 保持 0 = 处理完毕
        var src = wasmView(ptr, len);
        putSave(new Uint8Array(src.slice ? src.slice(0) : Array.prototype.slice.call(src)));
      };

      // 世界就绪：Rust 喊一声，这边就把门面收掉、HUD 亮起来（watchBoot 认这个）。
      env.mcl_booted = function () { window.__MCL_BOOTED = true; };

      // Rust 的报错/进度：既进 console（给开发者），也写到底部那一行（给读者）。
      env.mcl_log = function (ptr, len) {
        var m = decodeBytes(ptr, len);
        window.__MCL_LOGS = (window.__MCL_LOGS || []).concat(m);
        console.warn('[彩蛋] ' + m);
        // 启动阶段的话读者不用看，进 console 就够；崩了这种话必须写在门面上当原因——
        // 他手机上出问题时，那一行是我们唯一的线索。
        if (m.indexOf('崩') >= 0) {
          showFallback(m);
          try { say(m); } catch (e) {}
        } else if (m.indexOf('跑不了') >= 0) {
          try { say(m); } catch (e) {}
        }
      };

      env.mcl_save_pending = function () { return pending; };
      env.mcl_save_ready = function () { armLoadFallback(); return saveState; };

      // 存档一共有多少字节（没有存档返回 0）
      env.mcl_save_len = function () { return saveBytes ? saveBytes.length : 0; };

      // 把存档字节写进 Rust 给的缓冲区：从 offset 起最多拷 cap 字节，返回实际拷贝数。
      // Rust 按 offset 一轮一轮要，直到拿到 0 为止；0 也可能是「读完了」或「没有存档」。
      env.mcl_pull_save = function (ptr, cap, offset) {
        if (!saveBytes || !window.wasm_memory) return 0;
        var off = offset | 0;
        if (off < 0) off = 0;
        if (off >= saveBytes.length) return 0;
        var n = Math.min(saveBytes.length - off, cap | 0);
        if (n <= 0) return 0;
        var dst = wasmView(ptr, n);
        if (!dst.length) return 0;
        if (dst.length < n) n = dst.length;
        dst.set(saveBytes.subarray(off, off + n));
        return n;
      };

      // 「物品栏」面板的命令队列：页面塞一条，Rust 每帧 take 走一条，取走即清。
      // 命令号：1..9 选快捷格 / 100+n 背包第 n 格跟手上对调 / 200+k 合第 k 条配方 /
      // 300 切创造生存 / 400 面板开（暂停）/ 401 面板关 / 410+n 亮度档位 /
      // 6000+方块id*100+数量 给料（自动化用）。
      env.mcl_take_ui_cmd = function () {
        if (!uiQueue.length) return 0;
        var c = uiQueue.shift();
        return c | 0;
      };

      // 手机玩家跳 / 飞 / 换方块：DOM 按钮被点过时返回 1（slot 返回有符号增量，+1 下一个
      // / -1 上一个），取走即清。Rust 每帧调一次。
      env.mcl_take_jump = function () {
        var v = wantJump ? 1 : 0;
        wantJump = false;
        return v;
      };
      env.mcl_take_fly = function () {
        var v = wantFly ? 1 : 0;
        wantFly = false;
        return v;
      };
      env.mcl_take_slot = function () {
        var v = wantSlot;
        wantSlot = 0;
        return v;
      };
      // 桌面自由转向的三条：攒下来的鼠标位移（像素，取走即清）+ 「这一帧视角归页面推」
      env.mcl_take_look_x = function () {
        var v = lookDx;
        lookDx = 0;
        return v;
      };
      env.mcl_take_look_y = function () {
        var v = lookDy;
        lookDy = 0;
        return v;
      };
      env.mcl_free_look = function () {
        return lookFree ? 1 : 0;
      };
    },

    on_init: function () {}
  });
}

// ---------------------------------------------------------------------------
// 加载 wasm（自己 fetch 是为了能显示进度，然后交给胶水脚本的 load()）
// ---------------------------------------------------------------------------

// macroquad 的胶水在没有 WebGL 时只会弹一句英文 alert（微信里常被吞），然后 gl=null
// 全线崩，alert 还会把 setTimeout 冻住连判死都不跑。所以我们先自己预检，给一句中文。
function hasWebgl() {
  var made = [];
  var ok = false;
  try {
    var c2 = document.createElement('canvas');
    if (c2.getContext) {
      var g2 = c2.getContext('webgl2');
      if (g2) { made.push(g2); ok = true; }
      if (!ok) {
        var c1 = document.createElement('canvas');
        var g1 = c1.getContext ? c1.getContext('webgl') : null;
        if (g1) { made.push(g1); ok = true; }
      }
    }
  } catch (e) {
    ok = false; // 探不了就当能跑，别误杀：胶水那边真跑不起来我们还有 25 秒看门狗
  }
  // 浏览器同时活着的 WebGL 上下文有上限，探完立刻交还，别占着画布那一份
  for (var i = 0; i < made.length; i++) {
    try {
      var ext = made[i].getExtension('WEBGL_lose_context');
      if (ext && ext.loseContext) ext.loseContext();
    } catch (e2) {}
  }
  return ok;
}

// 胶水的 load() 两条分支最后都是 .catch(e => console.error(e))：编译失败、实例化失败、
// wasm 里 main() 抛出去的东西，全都只写进 console，页面上停在「编译 wasm」、按钮灰着点不动、
// 一个字的原因都没有（而且判死要求 __MCL_PENDING 有值，只有 window error 会置，所以永不判死）。
// 这里临时接管 console.error，把第一条错捞出来当原因，看门狗到点就还原。
var consoleErrorSaved = null;
var glueErr = '';

function takeOverConsoleError() {
  if (consoleErrorSaved || !window.console || typeof console.error !== 'function') return;
  consoleErrorSaved = console.error;
  console.error = function () {
    if (!glueErr) {
      var parts = [], i;
      for (i = 0; i < arguments.length; i++) {
        try { parts.push(arguments[i] && arguments[i].message ? String(arguments[i].message) : describeErr(arguments[i])); }
        catch (e) { parts.push('?'); }
      }
      glueErr = parts.join(' ');
      window.__MCL_GLUE_ERR = glueErr;
    }
    if (consoleErrorSaved) {
      try { consoleErrorSaved.apply(console, arguments); } catch (e) {}
    }
  };
}

function releaseConsoleError() {
  if (consoleErrorSaved === null) return;
  console.error = consoleErrorSaved;
  consoleErrorSaved = null;
}

// 进度条的分母：GitHub Pages 对 wasm 发 content-encoding: gzip + content-length（压缩后），
// 而 res.body.getReader() 数的是解压后的字节，照 Content-Length 算会到 1/3 就 100%。
// 取 Content-Length 和 manifest 里真实字节数的较大值：传输压得再狠也不会超过解压后的体积。
function progressTotal(res, totalHint) {
  var declared = 0, n;
  try {
    n = parseInt(res && res.headers && res.headers.get('Content-Length') || '', 10);
    if (n > 0) declared = n;
  } catch (e) {}
  return Math.max(declared, totalHint || 0);
}

function fetchWithProgress(url, totalHint) {
  return fetch(url).then(function (res) {
    if (!res.ok) throw new Error('HTTP ' + res.status + '（' + url + '）');
    var total = progressTotal(res, totalHint);
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

var glueStarted = false;
var watchdogDone = false;
var wasmUrl = 'mc-lite.wasm';

function startGame() {
  // 胶水没加载上（mq_js_bundle.js 404 / 被拦）就别白下 500 KB wasm，直接说清楚。
  // 不在页面刚开时就喊：那违反「用户没点之前别吓他」这条规矩，交给内联脚本判死。
  if (typeof window.load !== 'function') {
    showFallback('胶水脚本 mq_js_bundle.js 没加载上，这个彩蛋跑不起来。');
    return;
  }
  var btn = $('btn-play');
  if (btn) { btn.dataset.loaded = '1'; btn.disabled = true; btn.textContent = '加载中'; }
  var load = $('load');
  if (load) load.hidden = false;
  var elState = $('load-state');
  var elBytes = $('load-bytes');
  if (elBytes) elBytes.textContent = '0 / ? KB';

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
    wasmUrl = manifest && manifest.wasm ? manifest.wasm : 'mc-lite.wasm';
    if (elState) elState.textContent = '下载 wasm（' + fmtSize(totalHint) + '）';

    return fetchWithProgress(wasmUrl, totalHint);
  }).then(function (blob) {
    if (elState) elState.textContent = '编译 wasm：老手机可能要 10-20 秒，别退出、别切走';

    if (!hasWebgl()) {
      showFallback('这台机器的浏览器没有可用的 WebGL，画不出方块。');
      return;
    }

    takeOverConsoleError(); // 捞胶水管线里那些只写进 console 的失败

    // blob URL 只是优化（省掉胶水那一次本地 fetch）；做不出来就直接把原始地址交给胶水。
    var objUrl = '';
    try { objUrl = URL.createObjectURL(blob); } catch (e) { objUrl = ''; }
    if (!objUrl) {
      try { say('浏览器不给做临时地址，改用服务器上的 wasm 直接跑。'); } catch (e) {}
    }

    try {
      window.load(objUrl || wasmUrl);
    } catch (e) {
      if (!glueErr) glueErr = describeErr(e);
      showFallback('启动失败：' + (glueErr || describeErr(e)));
      releaseConsoleError();
      return;
    }
    glueStarted = true;
    startBootWatchdog(); // 必须无条件开始数：原来只有 window error 会置 PENDING，
                         // 编译失败一声不响时那条判死逻辑永远不触发。
  }).catch(function (err) {
    showFallback('加载失败：' + describeErr(err));
  });
}

function startBootWatchdog() {
  var started = Date.now();
  var timer = setInterval(function () {
    if (booted || watchdogDone) { clearInterval(timer); return; }
    var waited = Date.now() - started;
    if (waited >= BOOT_WATCHDOG_MS) {
      watchdogDone = true;
      clearInterval(timer);
      releaseConsoleError();
      if (window.__MCL_BOOTED) return; // 最后一刻起来了，算成功
      showFallback('wasm 没起来：' + (glueErr || '胶水只把错误写进了 console'));
    }
  }, 200);
}

// ---------------------------------------------------------------------------
// HUD 与门面
// ---------------------------------------------------------------------------

function showFallback(msg) {
  var f = $('fallback');
  var w = $('fallback-why');
  var btn = $('btn-play');
  var gate = $('gate');
  if (f) f.hidden = false;
  // 原因单独一行：手机上出问题时用户只要截图这一行就能告诉我到底哪儿不对。
  if (w) {
    w.hidden = false;
    w.textContent = '原因：' + (msg || window.__MCL_ERR || '没留下原因') +
      (window.__MCL_RES ? '｜另外 ' + window.__MCL_RES : '');
  }
  // 跑成功时 watchBoot 把整个 #gate 藏了；之后再出致命错（wasm 里崩了），
  // 原因就藏在被藏起来的父节点里 = 等于没写。这时候把门面重新显出来。
  if (gate && gate.hidden && window.__MCL_BOOTED) gate.hidden = false;
  // 以前这里把按钮 display:none 一藏了之，用户连重试的入口都没了。
  if (btn) {
    btn.style.display = '';
    btn.disabled = false;
    btn.textContent = '刷新重试';
    btn.onclick = function () { location.replace(retryHref()); };
  }
}

// 亮度三档的名字。数值（0 / 0.45 / 0.78）在 Rust 那边的 BRIGHT 表里，这边只管档位和文案。
var BRIGHT_NAMES = ['亮度：标准', '亮度：提高', '亮度：最高'];
var brightLevel = 0;
try { brightLevel = parseInt(localStorage.getItem('mcl.bright') || '0', 10) | 0; } catch (e) { brightLevel = 0; }
if (brightLevel !== 0 && brightLevel !== 1 && brightLevel !== 2) brightLevel = 0;

function brightLabel() {
  var b = $('btn-bright');
  if (b) b.textContent = BRIGHT_NAMES[brightLevel];
}

function setBright(n) {
  brightLevel = (n | 0) % BRIGHT_NAMES.length;
  if (brightLevel < 0) brightLevel += BRIGHT_NAMES.length;
  try { localStorage.setItem('mcl.bright', String(brightLevel)); } catch (e) {}
  uiCmd(410 + brightLevel);
  brightLabel();
}

window.__MCL_SET_BRIGHT = function (n) { setBright(n | 0); return brightLevel; };

// 手机切后台、系统回收显卡、驱动抽风都会把 WebGL 上下文弄丢，macroquad 不会自己恢复它：
// 画布就永远停在最后一帧或直接黑屏，看着像"游戏卡死"。接住事件、说清原因、给一键刷新，
// 别让它默默烂掉（存档每 3 秒存一次，刷新回来最多丢 3 秒）。
(function () {
  var c = $('glcanvas');
  if (!c || !c.addEventListener) return;
  try {
    c.addEventListener('webglcontextlost', function (ev) {
      if (ev && ev.preventDefault) ev.preventDefault();   // 拦下来才不给 GL 刷一堆错误
      window.__MCL_CTX_LOST = 1;
      showFallback('显卡的 WebGL 上下文断了（手机切后台、系统回收显卡、驱动抽风都会）。' +
        '存档每 3 秒存一次，刷新回来最多丢 3 秒。');
    }, false);
  } catch (e) {}
})();

// ---------------------------------------------------------------------------
// 物品栏：数据走 HUD 的第 10 段（Rust 推的，形状见 inv.rs 的 inv_text），
// 操作走命令队列（uiCmd）。这边只管画和点，规则全在 Rust 那边。
var uiQueue = [];
var invOpen = false;
var invText = '';

function uiCmd(c) {
  uiQueue.push(c | 0);
  if (uiQueue.length > 64) uiQueue.splice(0, uiQueue.length - 64);   // 卡住时别越堆越多
}

// 方块的格子颜色：从图集那套色里挑的代表色，纯粹是给面板看的，不参与渲染。
var BLOCK_COLOR = {
  1: '#68963e', 2: '#7a5637', 3: '#88888f', 4: '#d9cf9a', 5: '#386cb0', 6: '#6b4a2b',
  7: '#b58a4e', 8: '#4a782c', 9: '#e8eef6', 10: '#d6a842', 11: '#7a9a4e', 12: '#a8804a'
};

// 「C:1;B:0;S:2;N:1-草方块,...;H:1:8,...;G:...;R:7:4>6:1,...;A:1,0,...」 -> {C:'1',N:'...',...}
function parseInv(t) {
  var f = {};
  var parts = String(t).split(';');
  for (var i = 0; i < parts.length; i++) {
    var k = parts[i].charAt(0);
    f[k] = parts[i].slice(2);
  }
  return f;
}

function invNames(f) {
  var m = {};
  var items = String(f.N || '').split(',');
  for (var i = 0; i < items.length; i++) {
    var o = items[i].indexOf('-');
    if (o > 0) m[items[i].slice(0, o)] = items[i].slice(o + 1);
  }
  return m;
}

function invCells(f, names, key, from, to, sel, creative) {
  var out = '';
  var slots = String(f[key] || '').split(',');
  var base = key === 'H' ? 0 : 9;   // 背包那一段自己是 0 起数的，格子的全局下标要加回来
  for (var i = from; i < to; i++) {
    var pair = String(slots[i - base] || '0:0').split(':');
    var id = pair[0] | 0;
    var n = pair[1] | 0;
    var empty = id === 0 || (!creative && n === 0);
    var cmd = key === 'H' ? (i + 1) : (100 + (i - 9));
    out += '<button class="g-inv-cell" type="button" data-cmd="' + cmd + '" title="' +
      (empty ? '空着' : names[id] + ' ×' + n) + '"' +
      (empty ? ' data-empty="1"' : '') +
      (key === 'H' && i === sel ? ' data-sel="1"' : '') + '>' +
      (empty ? '' : '<span class="g-inv-chip" style="background:' + (BLOCK_COLOR[id] || '#999') + '"></span>') +
      '<span class="g-inv-name">' + (empty ? '' : (names[id] || '')) + '</span>' +
      '<span class="g-inv-n">' + (empty ? '' : (creative ? '∞' : n)) + '</span></button>';
  }
  return out;
}

// 底下那条快捷栏：同一份数据（HUD 第 10 段），跟物品栏面板不会各算各的。
// 只有色块和数量，名字在格子上面那一行 —— 9 格要挤进手机宽度，塞不下名字。
function hotCells(f, sel, creative) {
  var out = '';
  var slots = String(f.H || '').split(',');
  for (var i = 0; i < 9; i++) {
    var pair = String(slots[i] || '0:0').split(':');
    var id = pair[0] | 0;
    var n = pair[1] | 0;
    var empty = id === 0 || (!creative && n === 0);
    out += '<button class="g-hot-cell" type="button" data-slot="' + i + '"' +
      (i === sel ? ' data-sel="1"' : '') + '>' +
      '<i class="g-hot-chip" style="' + (empty ? '' : 'background:' + (BLOCK_COLOR[id] || '#999')) + '"></i>' +
      '<b class="g-hot-n">' + (empty ? '' : (creative ? '∞' : n)) + '</b></button>';
  }
  return out;
}

var hotLast = '';
function renderHot() {
  var box = $('hot-cells');
  if (!box || hotLast === invText) return;   // 没变就别碰 DOM（跟面板一条路子）
  hotLast = invText;
  var f = parseInv(invText);
  box.innerHTML = hotCells(f, (f.S | 0) - 1, f.C === '1');
}

function invRecipes(f, names) {
  var recs = String(f.R || '').split(',');
  var can = String(f.A || '').split(',');
  var out = '';
  for (var i = 0; i < recs.length; i++) {
    if (!recs[i]) continue;
    var arrow = recs[i].indexOf('>');
    var lhs = recs[i].slice(0, arrow).split(':');
    var outId = lhs[0] | 0, outN = lhs[1] | 0;
    var needTxt = recs[i].slice(arrow + 1).replace('*', '');
    var bench = recs[i].indexOf('*') >= 0;
    var parts = needTxt.split('+');
    var need = [];
    for (var j = 0; j < parts.length; j++) {
      var kv = parts[j].split(':');
      need.push((names[kv[0] | 0] || kv[0]) + ' ×' + (kv[1] | 0));
    }
    var ok = can[i] === '1';
    out += '<div class="g-inv-rec"><span class="g-inv-rec-text">' +
      '<span class="g-inv-need">' + need.join(' + ') + '</span> → ' +
      '<span class="g-inv-out">' + (names[outId] || outId) + ' ×' + outN + '</span>' +
      (bench ? ' <span class="g-inv-tag" data-on="' + (f.B === '1' ? '1' : '0') + '">' +
        (f.B === '1' ? '有工作台' : '需工作台') + '</span>' : '') +
      '</span><button class="g-inv-btn" type="button" data-cmd="' + (200 + i) + '"' +
      (ok ? '' : ' disabled') + '>合成</button></div>';
  }
  return out;
}

function renderInv() {
  var body = $('inv-body');
  if (!body) return;
  var f = parseInv(invText);
  var names = invNames(f);
  var creative = f.C === '1';
  var sel = (f.S | 0) - 1;
  var html = '<div class="g-inv-sec">手上（数字键 1-9 对应这排）</div><div class="g-inv-grid">' +
    invCells(f, names, 'H', 0, 9, sel, creative) + '</div>' +
    '<div class="g-inv-sec">背包</div><div class="g-inv-grid">' +
    invCells(f, names, 'G', 9, 36, sel, creative) + '</div>' +
    '<div class="g-inv-sec">合成' + (creative ? '（创造模式不看数量，合不成）' : '') + '</div>' +
    invRecipes(f, names);
  if (body.innerHTML !== html) body.innerHTML = html;   // 没变就别重写，免得白刷一遍布局
  var mb = $('inv-mode');
  if (mb) mb.textContent = creative ? '改成生存' : '改成创造';
  var tb = $('inv-bench');
  if (tb) {
    tb.textContent = f.B === '1' ? '旁边有工作台' : '旁边没工作台';
    tb.setAttribute('data-on', f.B === '1' ? '1' : '0');
  }
}

function setInv(open) {
  invOpen = !!open;
  var el = $('inv');
  if (el) el.hidden = !invOpen;
  uiCmd(invOpen ? 400 : 401);
  if (invOpen) renderInv();
  var c = $('glcanvas');
  if (c && c.focus) { try { c.focus(); } catch (e) {} }
}

function toggleInv() { setInv(!invOpen); }

// 自动化 / 调试用的给料：切到生存之后 __MCL_GIVE(3, 5) = 给 5 个石头
window.__MCL_GIVE = function (id, n) { uiCmd(6000 + ((id | 0) * 100) + (n | 0)); return 1; };
window.__MCL_INV_OPEN = function (on) { setInv(!!on); return invOpen ? 1 : 0; };

var HUD_IDS = [null, 'hot-held', 'hud-mode', 'hud-clock', 'hud-changed', 'hud-fps', 'hud-save', 'hud-note'];
var hudLast = {};

function tickHud() {
  var raw = window.__MCL_HUD;
  if (!raw) return;
  var parts = String(raw).split('|');
  for (var i = 0; i < HUD_IDS.length; i++) {
    var id = HUD_IDS[i];
    if (!id) continue;   // 第 1 段（第几格）不单独写了：底下快捷栏哪格亮着就是第几格
    var v = parts[i];
    if (v == null) continue;
    if (hudLast[id] === v) continue; // 没变就别碰 DOM：帧率那格每 0.2 秒重写一次纯属白刷，
                                     // 还会和胶水的 getBoundingClientRect 互相逼出强制同步布局
    hudLast[id] = v;
    var el = $(id);
    if (!el) continue;
    if (el.textContent !== v) el.textContent = v;
    if (id === 'hud-note') el.hidden = (v === ''); // 备注平时是空的，别挂个空块在画面上
  }
  // 第 10 段是物品栏的数据。面板开着才画，关着只留一份最新的等点开用。
  var it = parts[9];
  if (it != null && it !== invText) {
    invText = String(it);
    if (invOpen) renderInv();
    renderHot();   // 底下那条快捷栏跟着同一份数据刷（面板关着也得刷，它一直在画面上）
  }
}

function fireResize() {
  // 胶水的 resize 处理挂在 window.onresize 上；派发一个 resize 事件就会走到它，
  // 比直接调那个函数稳（不用赌胶水已经赋过值），也不打断它自己的处理链。
  try {
    if (typeof window.dispatchEvent === 'function' && typeof Event === 'function') {
      window.dispatchEvent(new Event('resize'));
      return;
    }
  } catch (e) {}
  try {
    var ev = document.createEvent('Events');
    ev.initEvent('resize', true, true);
    window.dispatchEvent(ev);
    return;
  } catch (e) {}
  try { if (typeof window.onresize === 'function') window.onresize(); } catch (e) {}
}

var resizeCatchup = false;
function afterBootResizeCatchup() {
  if (resizeCatchup) return;
  resizeCatchup = true;

  // (a) Web 字体晚到会让顶栏/页脚变高，画布被轻微拉伸，而没人喊 resize
  try {
    if (document.fonts && document.fonts.ready && typeof document.fonts.ready.then === 'function') {
      document.fonts.ready.then(function () { fireResize(); }, function () {});
    }
  } catch (e) {}

  // (b) iOS 旋转时那一瞬 clientWidth 常常还是旧值：胶水是在 wasm 起来之后才给
  // window.onresize 赋值的，所以我们在旋转后再补两轮，让胶水自己去发现尺寸变了。
  if (window.addEventListener) {
    try {
      window.addEventListener('orientationchange', function () {
        setTimeout(fireResize, 250);
        setTimeout(fireResize, 1000);
      }, false);
    } catch (e) {}
  }

  // (c) 页脚文案变长/字体到位也会改高度：起来之后再补两轮，兜住没人喊的情况
  setTimeout(fireResize, 600);
  setTimeout(fireResize, 2200);
}

function focusCanvas() {
  // macroquad 的键盘事件挂在 canvas 上。用户点过「点开玩」之后焦点还在按钮上，
  // 于是跑起来之后 WASD 完全没反应。
  var c = $('glcanvas');
  if (!c || typeof c.focus !== 'function') return;
  try { c.focus({ preventScroll: true }); }
  catch (e) { try { c.focus(); } catch (e2) {} }
}

// 在画布外面松手也要算松手。胶水的 mouseup/pointerup 只绑在画布上，跑着的时候在顶栏、
// 页脚或者画布外头松的手，macroquad 那边就永远看不到松手：鼠标键状态卡在「还按着」，
// 之后光是动一下鼠标，视角就自己转起来了（实测：事后挪六下鼠标，画面中心真的转掉了）。
// 这里在 window 上补一道，把不在画布上的松手原样转交给画布；画布自己收到的不转，免得处理两遍。
function forwardOutsideUp(ev) {
  var c = $('glcanvas');
  if (!c || !ev) return;
  var t = ev.target || ev.srcElement;
  if (t === c) return;
  var mk = null;
  try {
    if (ev.type === 'mouseup' && typeof window.MouseEvent === 'function') {
      mk = new MouseEvent('mouseup', {
        bubbles: true, cancelable: true, clientX: ev.clientX, clientY: ev.clientY,
        button: ev.button, buttons: ev.buttons
      });
    } else if (ev.type === 'pointerup' && typeof window.PointerEvent === 'function') {
      mk = new PointerEvent('pointerup', {
        bubbles: true, cancelable: true, clientX: ev.clientX, clientY: ev.clientY,
        button: ev.button, buttons: ev.buttons, pointerId: ev.pointerId,
        pointerType: ev.pointerType || 'mouse', isPrimary: ev.isPrimary !== false
      });
    }
  } catch (e) { mk = null; }
  if (!mk) return;
  try { c.dispatchEvent(mk); } catch (e) {}
}

// ---------------------------------------------------------------------------
// 桌面转向：鼠标一动就转。这是用户明确要求的（「在电脑上运行就用鼠标移动控制视角」），
// 原来是「按住拖动才转」，鼠标光动着什么都不发生。
//
// 光标能走的距离有限（撞到画布边就没得动了），所以能指针锁定就锁：锁上之后浏览器直接给
// movementX/movementY，转多少圈都不受限。引擎不认指针锁定（老 Safari、一些微信内核）就退回
// 按 clientX/clientY 差分，那种情况下光标离开画布就不再转 —— 他离开画布多半是去点顶上那排
// 存档按钮，不该顺手把视角带走。
// 按着鼠标的时候不推视角：那一下是要挖/放，手难免抖，不然每挖一对方位就跟着抖一下。
var LOOK_SENS_PER_PX = Math.PI / 700; // 和以前按住拖动同一档：整屏 180° 约 700 像素
var lookDx = 0, lookDy = 0, lookFree = false, lookLocked = false;
var lookLastX = 0, lookLastY = 0, lookHaveLast = false;

function isTouchish() {
  if ('ontouchstart' in window) return true;
  if ((navigator.maxTouchPoints || 0) > 0) return true;
  return !!(window.matchMedia && window.matchMedia('(any-pointer: coarse)').matches);
}

function canvasHasCursor(ev) {
  var c = $('glcanvas');
  if (!c || typeof ev.clientX !== 'number') return false;
  var r = c.getBoundingClientRect();
  return ev.clientX >= r.left && ev.clientX <= r.right && ev.clientY >= r.top && ev.clientY <= r.bottom;
}

// 攒位移给 Rust 每帧取：攒多了就是转得快一点，不会丢动作。
function pushLook(ev) {
  if (!lookFree || !window.__MCL_BOOTED || !ev) return;
  if (ev.buttons !== 0) { lookHaveLast = false; return; }     // 按着的时候不转向
  var dx = 0, dy = 0;
  if (lookLocked && typeof ev.movementX === 'number' && (ev.movementX !== 0 || ev.movementY !== 0)) {
    dx = ev.movementX;
    dy = ev.movementY;
  } else {
    if (!lookLocked && !rotMode && !canvasHasCursor(ev)) { lookHaveLast = false; return; }
    if (!lookHaveLast) {
      lookLastX = ev.clientX; lookLastY = ev.clientY; lookHaveLast = true;
      return;
    }
    dx = ev.clientX - lookLastX;
    dy = ev.clientY - lookLastY;
    lookLastX = ev.clientX;
    lookLastY = ev.clientY;
  }
  if (!dx && !dy) return;
  lookDx += dx;
  lookDy += dy;
  if (lookDx > 4000) lookDx = 4000;   // 极端情况下别把一帧的转向撑到离谱
  if (lookDx < -4000) lookDx = -4000;
  if (lookDy > 4000) lookDy = 4000;
  if (lookDy < -4000) lookDy = -4000;
}

function requestLookLock() {
  var c = $('glcanvas');
  if (!c || !c.requestPointerLock) return;
  try { c.requestPointerLock(); } catch (e) {}
}

// ---------------------------------------------------------------------------
// 全屏 / 横屏：按引擎给不给 API 走三条路。
//   1) 有元素全屏 API（桌面 Chrome/Edge/Firefox、安卓 Chrome、平板 Safari）：整页 requestFullscreen，
//      进去以后 screen.orientation.lock('landscape') —— 安卓上这就自动横屏，桌面自己会忽略这句。
//   2) 压根没有元素全屏（iPhone 的 Safari、微信 iOS 内核只给视频）：退回 CSS 横屏，
//      整页按「视口宽高对调」摆好再转 90°，看着就是横屏。
//   3) 两个都不给（个别 WebView）：按钮按下去什么也不变，不装模作样。
// 横屏模式下画布被 CSS 转了 90°，而胶水算输入用的是 clientX/clientY 减画布的
// getBoundingClientRect（mq_js_bundle.js 里的 mouse_relative_position），所以在 window 的捕获相
// 把坐标改写成「没转的时候胶水该看见的值」：CSS rotate(90deg) 把局部 (x,y) 画到屏幕上是 (−y, x)，
// 反解就是 x = 屏幕y − 中心y、y = 中心x − 屏幕x，再加回画布没转时的左上角。
// 换算装在 MouseEvent.prototype / Touch.prototype 的 getter 上（不是往事件对象上盖，那样胶水
// 在我们读之前就取走原始值了），所以后面所有读 clientX 的人 —— 胶水、pushLook、转交给画布的那
// 份合成事件 —— 拿到的都是换算过的，不用每一处各修一遍。
var rotMode = false, rotRect = null, rotBusy = false;

function fsApi() {
  var de = document.documentElement;
  return de.requestFullscreen || de.webkitRequestFullscreen || de.webkitRequestFullScreen ||
    de.msRequestFullscreen || null;
}

function fsElement() {
  return document.fullscreenElement || document.webkitFullscreenElement ||
    document.msFullscreenElement || null;
}

function exitFs() {
  var ex = document.exitFullscreen || document.webkitExitFullscreen ||
    document.webkitCancelFullScreen || document.msExitFullscreen;
  if (ex) { try { ex.call(document); } catch (e) {} }
}

function lockLandscape(on) {
  try {
    var so = window.screen && window.screen.orientation;
    if (!so) return;
    if (on && typeof so.lock === 'function') {
      var p = so.lock('landscape');
      if (p && typeof p['catch'] === 'function') p['catch'](function () {});   // 桌面不认，忽略
    } else if (!on && typeof so.unlock === 'function') {
      so.unlock();
    }
  } catch (e) {}
}

function refreshRotRect() {
  try { rotRect = $('glcanvas').getBoundingClientRect(); } catch (e) { rotRect = null; }
}

// 屏幕坐标 → 画布没转时胶水该看到的坐标
function unrotate(cx, cy) {
  var rc = rotRect;
  if (!rc || !(rc.width > 0) || !(rc.height > 0)) return [cx, cy];
  var cxm = rc.left + rc.width / 2, cym = rc.top + rc.height / 2;
  // 没转时画布宽 = 转了之后的 bounding 高，没转时画布高 = 转了之后的 bounding 宽
  return [rc.left + (cy - cym) + rc.height / 2, rc.top + (cxm - cx) + rc.width / 2];
}

// 改原型上的 getter，不是改事件对象：事件对象上盖一层 getter 实测不顶用 —— 胶水在我们之前
// 就读走了原始屏幕坐标（现象是横屏里拖一下人往后走、视角一点不动，因为那串坐标落进了左半屏
// 的摇杆）。原型上改一次，谁什么时候读都是换算过的，跟监听注册先后没关系。
// PointerEvent.prototype 继承 MouseEvent.prototype，没有自己的 clientX，改 MouseEvent 就够。
var inputPatch = null;

function fixInputCoords(on) {
  if (on) {
    if (inputPatch) return true;
    var saved = [];
    var hosts = [];
    if (window.MouseEvent) hosts.push(window.MouseEvent.prototype);
    if (window.Touch) hosts.push(window.Touch.prototype);
    for (var i = 0; i < hosts.length; i++) {
      var h = hosts[i];
      try {
        var dx = Object.getOwnPropertyDescriptor(h, 'clientX');
        var dy = Object.getOwnPropertyDescriptor(h, 'clientY');
        if (!dx || typeof dx.get !== 'function' || !dx.configurable) continue;
        if (!dy || typeof dy.get !== 'function' || !dy.configurable) continue;
        var gx = dx.get, gy = dy.get;
        Object.defineProperty(h, 'clientX', {
          get: (function (gx, gy) {
            return function () { return unrotate(gx.call(this), gy.call(this))[0]; };
          })(gx, gy),
          configurable: true, enumerable: dx.enumerable
        });
        Object.defineProperty(h, 'clientY', {
          get: (function (gx, gy) {
            return function () { return unrotate(gx.call(this), gy.call(this))[1]; };
          })(gx, gy),
          configurable: true, enumerable: dy.enumerable
        });
        saved.push([h, 'clientX', dx]);
        saved.push([h, 'clientY', dy]);
      } catch (e) {}
    }
    inputPatch = saved.length ? saved : false;
    window.__MCL_ROT_PATCH = saved.length ? 1 : 0;      // 装没装上，测试和排查都看这个
    return !!saved.length;
  }
  if (inputPatch) {
    for (var k = 0; k < inputPatch.length; k++) {
      var s = inputPatch[k];
      try { Object.defineProperty(s[0], s[1], s[2]); } catch (e) {}
    }
  }
  inputPatch = null;
  window.__MCL_ROT_PATCH = 0;
  return false;
}

function fullLabel() {
  var b = $('btn-full');
  if (!b) return;
  b.textContent = (fsElement() || rotMode) ? '退出全屏' : '全屏';
}

function applyRotate(on) {
  var b = document.body;
  if (!b) return;
  rotMode = !!on;
  fixInputCoords(rotMode);
  if (rotMode) {
    var vw = window.innerWidth, vh = window.innerHeight;   // 现在是竖着的
    if (b.classList) b.classList.add('g-rot');
    b.style.width = vh + 'px';        // 横屏的宽 = 竖屏的高（vh/vw 在 iOS 上算不准，直接量像素）
    b.style.height = vw + 'px';
    b.style.left = vw + 'px';         // 绕左上角转 90° 会整块跑到视口左边，挪回来正好铺满
    b.style.top = '0px';
  } else {
    if (b.classList) b.classList.remove('g-rot');
    b.style.width = ''; b.style.height = ''; b.style.left = ''; b.style.top = '';
  }
  refreshRotRect();
  fireResize();
  setTimeout(function () { refreshRotRect(); }, 120);   // 胶水改完画布尺寸，rect 才是新的
  setTimeout(function () { fireResize(); refreshRotRect(); }, 500);
  fullLabel();
}

function enterFullscreen() {
  var req = fsApi();
  if (req) {
    try {
      var p = req.call(document.documentElement);
      if (p && typeof p.then === 'function') {
        p.then(function () { lockLandscape(true); fullLabel(); },
               function () { applyRotate(true); });
      } else {
        // 老引擎不给 promise：等 fullscreenchange，等不到就当不成，退回横屏
        setTimeout(function () { if (!fsElement() && !rotMode) applyRotate(true); }, 800);
      }
      return;
    } catch (e) { applyRotate(true); return; }
  }
  applyRotate(true);
}

function leaveFullscreen() {
  if (fsElement()) { exitFs(); lockLandscape(false); }
  if (rotMode) applyRotate(false);
  fullLabel();
}

// 自动化用：无头浏览器里没有用户手势，走不到真全屏，这条直接开/关横屏模式好测坐标换算
window.__MCL_SET_ROTATE = function (on) { applyRotate(!!on); return rotMode ? 1 : 0; };

function watchBoot() {
  var timer = setInterval(function () {
    if (!window.__MCL_BOOTED) return;
    clearInterval(timer);
    booted = true;
    releaseConsoleError(); // 起来了，console.error 还给原主

    var gate = $('gate');
    if (gate) gate.hidden = true;
    // 起得慢的机器（wasm 编译要几秒）会被那个 2.5 秒的兜底先抢跑一步，
    // 真跑起来之后必须把它收掉，否则页面上一直挂着「这台机器的浏览器跑不了」。
    // 连「原因：…」那一行和进度条也得一起收掉，别留在画面上。
    var fb = $('fallback');
    if (fb) fb.hidden = true;
    var fbwhy = $('fallback-why');
    if (fbwhy) fbwhy.hidden = true;
    var load = $('load');
    if (load) load.hidden = true;
    var hud = $('hud');
    if (hud) hud.hidden = false;
    var cross = $('cross');
    if (cross) cross.hidden = false;

    ['btn-export', 'btn-import', 'btn-reset'].forEach(function (id) {
      var b = $(id);
      if (b) b.disabled = false;
    });

    // 触屏设备换一套操作提示，并把 HUD 上的跳/飞/换方块按钮显出来
    // 触屏判据别只盯 ontouchstart：带触屏的笔记本、平板、一些国产 WebView 要么压根没这个属性、
    // 要么它有但不能用。(any-pointer: coarse) 更实在 —— 真能用手指头点的设备才叫 coarse。
    var touchish = isTouchish();
    lookFree = !touchish;   // 桌面才走「移动鼠标就转向」；触屏走右半屏拖动
    var hd = $('hint-desktop');
    var ht = $('hint-touch');
    if (touchish) {
      if (hd) hd.hidden = true;
      if (ht) ht.hidden = false;
      var acts = $('hud-btns');
      if (acts) acts.hidden = false;
    }

    tickHud(); // 先把已有的一帧写上去，别空 200 毫秒
    setInterval(tickHud, 200);

    focusCanvas();
    afterBootResizeCatchup();
  }, 200);
}

// ---------------------------------------------------------------------------
// 导入文件的体检：别把「任何以 MCL1 开头的东西」当存档写进存储
// ---------------------------------------------------------------------------

function checkSaveBytes(bytes) {
  if (!bytes || bytes.length < 20) return '太短了，不像存档';
  if (bytes.length > IMPORT_MAX_BYTES) return '太大（' + fmtSize(bytes.length) + '），不像存档';
  var magic = String.fromCharCode(bytes[0], bytes[1], bytes[2], bytes[3]);
  if (magic !== 'MCL1') return '文件头不是 MCL1';
  var ver = bytes[4];
  if (ver !== 1 && ver !== 2) return '版本不认识（' + ver + '）';
  if (ver === 1) {
    // 头 17 字节：MCL1 + 版本 + 种子(4) + 原始长度(4) + 编码后长度(4)
    var bodyLen = bytes[13] + bytes[14] * 256 + bytes[15] * 65536 + bytes[16] * 16777216;
    if (bytes.length < 17 + bodyLen) return '存档被截断了';
  }
  return '';
}

// ---------------------------------------------------------------------------
// 存档写入 / 清档之后刷新：等存储真写完，别让 120 毫秒的定时器赌运气
// ---------------------------------------------------------------------------

function reloadAfterWrite(writePromise, doingMsg, failMsg) {
  suppressSave = true; // 关键：刷新之前别让还活着的 wasm 再写一遍，把结果盖掉
  say(doingMsg);
  var settled = false;

  function finish(wrote, why) {
    if (settled) return;
    settled = true;
    if (!wrote) {
      var full = failMsg + (why ? '（' + why + '）' : '');
      // 刷新会吃掉页脚那一行，所以把原因留到下一次进页面再说一遍
      try { say(full); } catch (e) {}
      noteLater(full);
    }
    if (document.fonts && document.fonts.ready && typeof document.fonts.ready.then === 'function') {
      // 存储写完了不等于地址能换：iOS 在 wasm 主循环忙的时候可能把导航攒着
      document.fonts.ready.then(reloadNow, reloadNow);
      setTimeout(reloadNow, 700);
    } else {
      reloadNow();
    }
  }

  function reloadNow() {
    if (reloadDone) return;
    reloadDone = true;
    try { location.replace(retryHref()); }
    catch (e) { try { location.reload(); } catch (e2) {} }
  }

  writePromise.then(
    function (wrote) { finish(wrote, ''); },
    function (err) { finish(false, describeErr(err)); }
  );
  setTimeout(function () { finish(false, ''); }, RELOAD_FALLBACK_MS + 500);
}

// ---------------------------------------------------------------------------
// 按钮
// ---------------------------------------------------------------------------

function wireButtons() {
  var play = $('btn-play');
  if (play) play.addEventListener('click', function () {
    if (play.dataset.loaded) return;
    startGame();
  });

  var ex = $('btn-export');
  if (ex) ex.addEventListener('click', function () {
    if (!saveBytes) { say('还没有存档：先在岛上挖两下，等 HUD 上显示「已存档」。'); return; }
    var url = '';
    try {
      url = URL.createObjectURL(new Blob([saveBytes], { type: 'application/octet-stream' }));
    } catch (e) { url = ''; }
    if (!url) { say('这台机器做不出可下载的文件（createObjectURL 失败）。'); return; }
    var a = document.createElement('a');
    a.href = url;
    a.download = '方块存档-' + stamp() + '.mcl';
    a.target = '_blank'; // WKWebView 不接管 blob 下载：走 iOS 的 QuickLook / 分享面板
    a.rel = 'noopener';
    a.style.display = 'none';
    document.body.appendChild(a);
    a.click();
    // 分享面板要的时间远超 2 秒：节点推到下一帧再摘，地址延到 30 秒再销毁
    setTimeout(function () { try { document.body.removeChild(a); } catch (e) {} }, 0);
    setTimeout(function () { try { URL.revokeObjectURL(url); } catch (e) {} }, 30000);
    say('已生成 ' + fmtSize(saveBytes.length) + ' 的文件。如果没弹出来，多半是微信不接下载——' +
        '去电脑或手机浏览器里打开这个页面再导。');
  });

  var im = $('btn-import');
  var file = $('file-import');
  if (im && file) im.addEventListener('click', function () {
    try { file.click(); }
    catch (e) { say('这台机器不让程序打开文件选择器，请在系统浏览器里打开这个页面再导入。'); }
  });
  if (file) file.addEventListener('change', function () {
    var f = file.files && file.files[0];
    if (!f) return;
    var rd = new FileReader();
    rd.onload = function () {
      var bytes = new Uint8Array(rd.result);
      var why = checkSaveBytes(bytes);
      if (why) {
        say('这个文件不像这个游戏的存档：' + why);
        try { file.value = ''; } catch (e) {} // 同一个文件再选一次也要重新走一遍
        return;
      }
      saveBytes = bytes;
      saveState = STATE_READY;
      // 世界在 wasm 里是常驻内存的，换存档最省事的办法是整页重来
      reloadAfterWrite(putSave(bytes), '正在用导入的文件重建世界…', '导入没写进存储，稍后再导一次试试');
      try { file.value = ''; } catch (e) {}
    };
    rd.onerror = function () { say('这个文件读不出来。'); };
    rd.readAsArrayBuffer(f);
  });

  var rs = $('btn-reset');
  if (rs) rs.addEventListener('click', function () {
    reloadAfterWrite(clearSave(), '已清掉本地存档，正在按种子重生成一座新岛…', '清档没清干净');
  });

  // 手机端入口：HUD 上的小按钮置标志，Rust 每帧 take 走（取走即清）
  wireHudTap('btn-jump', function () { wantJump = true; });
  wireHudTap('btn-fly', function () { wantFly = true; });
  wireHudTap('btn-inv', function () { toggleInv(); });

  // 面板里的点法：格子带 data-cmd，一个委托监听全接住（每 0.2 秒可能整块重画，
  // 一个个挂监听会越挂越多）。
  var invEl = $('inv');
  if (invEl && invEl.addEventListener) {
    invEl.addEventListener('click', function (ev) {
      var t = ev.target;
      while (t && t !== invEl) {
        if (t.getAttribute) {
          var c = t.getAttribute('data-cmd');
          if (c != null) { uiCmd(parseInt(c, 10) | 0); renderInv(); return; }
          if (t.id === 'inv-close' || t === $('inv-back')) { setInv(false); return; }
          if (t.id === 'inv-mode') { uiCmd(300); renderInv(); return; }
        }
        t = t.parentNode;
      }
    }, false);
  }

  // 亮度：三档，切了就发命令 410+n，曲线在着色器那边（u_bright）。
  // 存 localStorage 不进存档 —— 这是显示偏好，换台设备该跟着浏览器走，不该跟着世界走。
  var hot = $('hot-cells');
  if (hot && hot.addEventListener) hot.addEventListener('click', function (ev) {
    var t = ev && ev.target ? ev.target : null;
    // 点在里面那个色块 / 数字上也算点这一格，所以要往上找到带 data-slot 的那层
    while (t && t !== hot && !(t.getAttribute && t.getAttribute('data-slot'))) t = t.parentNode;
    if (!t || !t.getAttribute) return;
    var i = parseInt(t.getAttribute('data-slot'), 10);
    if (i >= 0 && i < 9) uiCmd(i + 1);   // 命令号 1..9 = 选第几格，规则在 Rust 那边
  }, false);
  renderHot();

  var bb = $('btn-bright');
  if (bb && bb.addEventListener) bb.addEventListener('click', function () { setBright(brightLevel + 1); }, false);
  brightLabel();
  uiCmd(410 + brightLevel);   // 开局把存下来的档位告诉 wasm（队列它会一帧帧取走）

  // 电脑上：E 开关物品栏，Esc 也关（跟真全屏那条不冲突——全屏时 Esc 先解指针锁定）
  if (window.addEventListener) {
    try {
      window.addEventListener('keydown', function (ev) {
        var k = ev && ev.key ? String(ev.key).toLowerCase() : '';
        var code = ev && ev.keyCode ? ev.keyCode : 0;
        if (k === 'e' || code === 69) { toggleInv(); return; }
        if (k === 'b' || code === 66) { setBright(brightLevel + 1); return; }
        if (invOpen && (k === 'escape' || code === 27)) setInv(false);
      }, false);
    } catch (e) {}
  }

  // 点过 HUD 上的按钮之后焦点会留在按钮上，键盘（WASD）就又哑了；把焦点还给画布
  var c = $('glcanvas');
  if (c && c.addEventListener) {
    c.addEventListener('pointerdown', focusCanvas, false);
    c.addEventListener('mousedown', focusCanvas, false);
  }

  // 桌面自由转向：鼠标一动就转（见 pushLook）。只挂 mousemove 就够，
  // 再挂 pointermove 会和它各算一遍、转向翻倍；触屏那边 lookFree 本来就是 false。
  if (window.addEventListener) {
    try {
      document.addEventListener('mousemove', pushLook, false);
      document.addEventListener('pointerlockchange', function () {
        lookLocked = (document.pointerLockElement === $('glcanvas'));
        lookHaveLast = false;
      }, false);
      document.addEventListener('pointerlockerror', function () { lookLocked = false; }, false);
      var gl0 = $('glcanvas');
      if (gl0 && gl0.addEventListener) {
        gl0.addEventListener('mousedown', function (ev) {
          if (lookFree && !lookLocked && ev.button === 0) requestLookLock();
        }, false);
      }
    } catch (e) {}
  }

  // 全屏 / 横屏（见上面那一段）。坐标换算的监听挂在 window 的捕获相：这样比画布上的
  // 处理更早，胶水读到的就已经是换算过的坐标了。
  if (window.addEventListener) {
    try {
      var bf = $('btn-full');
      if (bf && bf.addEventListener) {
        bf.addEventListener('click', function (ev) {
          try { if (ev && ev.preventDefault) ev.preventDefault(); } catch (e) {}
          if (fsElement() || rotMode) leaveFullscreen(); else enterFullscreen();
        }, false);
      }
      ['fullscreenchange', 'webkitfullscreenchange', 'msfullscreenchange'].forEach(function (kind) {
        window.addEventListener(kind, function () {
          if (fsElement()) lockLandscape(true); else lockLandscape(false);
          fullLabel();
        }, false);
      });
      // Esc：真全屏浏览器自己会退，横屏模式得我们自己退
      window.addEventListener('keydown', function (ev) {
        if (rotMode && (ev.key === 'Escape' || ev.keyCode === 27)) leaveFullscreen();
      }, false);
      // 转屏/工具条收起会改视口尺寸，横屏模式要按新尺寸重新摆一遍
      window.addEventListener('resize', function () {
        if (!rotMode || rotBusy) return;
        rotBusy = true;
        applyRotate(true);
        setTimeout(function () { rotBusy = false; }, 250);
      }, false);
      window.addEventListener('orientationchange', function () {
        if (!rotMode) return;
        setTimeout(function () { if (rotMode) applyRotate(true); }, 300);
      }, false);
    } catch (e) {}
  }

  // 捕获相监听：画布外的松手转交给画布（见 forwardOutsideUp）
  if (window.addEventListener) {
    try {
      window.addEventListener('mouseup', forwardOutsideUp, true);
      window.addEventListener('pointerup', forwardOutsideUp, true);
    } catch (e) {}
  }
}

function wireHudTap(id, onPress) {
  var el = $(id);
  if (!el) return;
  var fired = false;
  function press(ev) {
    if (ev && ev.preventDefault) { try { ev.preventDefault(); } catch (e) {} }
    if (ev && ev.stopPropagation) { try { ev.stopPropagation(); } catch (e) {} }
    onPress();
    fired = true;
    focusCanvas();
    setTimeout(function () { fired = false; }, 400);
  }
  // iOS 13 及更早：touchend 里 window.open/location.assign 会被弹窗拦截器吃掉，
  // 必须待在 touch 同步派发的那一帧里。桌面没有 touch 事件，退回 click。
  if ('ontouchstart' in window && el.addEventListener) {
    el.addEventListener('touchend', press, false);
    el.addEventListener('click', function (ev) {
      if (fired) return;
      press(ev);
    }, false);
  } else if (el.addEventListener) {
    el.addEventListener('click', press, false);
  }
}

// ---------------------------------------------------------------------------
// 起手
// ---------------------------------------------------------------------------

registerBridge();
loadSave();
wireButtons();
drainNote();

// 起来之后要收门面、亮 HUD、开 tickHud、把焦点还给画布 —— watchBoot 定义了却没人调过：
// 于是门面永远盖着、HUD 八格永远停在「等待 / 06:00 / 读取存档」、#cross 和手机上的跳飞按钮
// 也一直不出来。无头实测：整场跑完 hud 相关的 DOM 改动次数是 0。
watchBoot();

// 无头浏览器截图 / 自动化测试用：地址后加 ?autoplay=1 就直接开跑，不用人去点按钮。
// 胶水没加载上不在这里下结论（startGame 里会说清楚），没点之前也别吓用户。
if (typeof window.load === 'function' && /[?&]autoplay=1/.test(location.search)) startGame();
