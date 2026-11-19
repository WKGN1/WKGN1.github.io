#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""方块彩蛋回归测试：把两个审查清单一一验掉。

用法：python3 /tmp/gqa.py            # 全跑
      python3 /tmp/gqa.py base touch # 只跑指定场景

只测 dist/（构建产物），所以要先 `bash game/build-wasm.sh && python3 build.py`。
信号都来自页面自己暴露的东西：__MCL_BOOTED / __MCL_HUD（9 段，第 9 段是我要求加的
调试位姿）/ __MCL_ERR / __MCL_RES；画面是否真的重画用 readPixels 读中心像素求和，
为此在页面脚本跑起来之前把 getContext 抢过来、强行开 preserveDrawingBuffer（只测试用）。

判"画面变了没有"不能只看绝对差：昼夜每 360 秒走一轮，光照本身会让整幅图变。
所以每个会话先测一次"什么都不做 2 秒"的漂移基线，再拿 3 倍基线当阈值。
"""
import base64
import functools
import json
import math
import os
import re
import shutil
import sys
import threading
import time
import urllib.request
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

BLOG = "/run/media/suyzz/e2270998-5ad9-4aef-860c-ba06f27ea0a9/strata/dsh/dsh_wechat/blog"
DIST = os.path.join(BLOG, "dist")
DRIVER = os.environ.get("WDDRIVER", "http://127.0.0.1:4444")
sys.path.insert(0, BLOG)
import wdshot  # noqa: E402

CHECKS = []          # (场景, 名称, ok, 说明)
SCENES = {}
SESSIONS = []        # 所有开过的会话：main 收尾统一关，异常中途也不会漏（第一版漏过一次，
                     # 结果残留的 Firefox 占着 geckodriver 唯一的会话位，下一轮直接起不来会话）
ONLY = sys.argv[1:]


def log(scene, name, ok, note=""):
    CHECKS.append((scene, name, ok, note))
    mark = "✓" if ok is True else ("~" if ok is None else "✗")
    print("  %s %s｜%s%s" % (mark, name, note, ""), flush=True)


# ───────────────────────────────── 会话与工具
class Sess:
    def __init__(self, prefs=None, w=1280, h=800, hook=""):
        self.prof = os.path.join(wdshot.PROFILE_ROOT, "ffprof-gqa-%d" % (int(time.time() * 1000) % 10 ** 9))
        os.makedirs(self.prof, exist_ok=True)
        p = {"privacy.resistFingerprinting": False}
        if prefs:
            p.update(prefs)
        caps = {"capabilities": {"alwaysMatch": {
            "browserName": "firefox", "pageLoadStrategy": "none", "unhandledPromptBehavior": "ignore",
            "moz:firefoxOptions": {"args": ["-headless", "-no-remote", "-profile", self.prof,
                                            "-width", str(w), "-height", str(h)], "prefs": p}},
            "firstMatch": [{}]}, }
        self.sid = wdshot.call(DRIVER, "POST", "/session", caps)["value"]["sessionId"]
        self.closed = False
        self.hook = hook
        SESSIONS.append(self)

    def ev(self, script, *args):
        return wdshot.call(DRIVER, "POST", "/session/%s/execute/sync" % self.sid,
                           {"script": script, "args": list(args)})["value"]

    def go(self, url):
        wdshot.call(DRIVER, "POST", "/session/%s/url" % self.sid, {"url": url}, timeout=120)
        if self.hook:
            self.install()

    def install(self):
        """新文档一活过来就装钩子。pageLoadStrategy:none 下 POST /url 一回来就 execute，
           文档可能还没提交，钩子会装进上一个文档里白装（第一版就被这个坑过）。"""
        t0 = time.time()
        while time.time() - t0 < 8:
            try:
                if self.ev("return location.pathname.indexOf('/game/') >= 0 && !window.__GQA_HOOKED ? 1 : 0;"):
                    self.ev(self.hook)
                    return True
            except BaseException:
                pass
            time.sleep(0.02)
        return False

    def close(self):
        if self.closed:
            return
        self.closed = True
        for _ in range(3):
            try:
                wdshot.call(DRIVER, "DELETE", "/session/%s" % self.sid, timeout=15)
                break
            except BaseException:
                time.sleep(0.4)
        shutil.rmtree(self.prof, ignore_errors=True)


HOOK = r"""
if (window.__GQA_HOOKED) return '已在';
window.__GQA_HOOKED = 1;
window.__CERR = [];
window.__CWARN = [];
window.__CLOG = [];
window.__HINTS = [];
(function(){
  var cw = console.warn, ce = console.error, cl = console.log;
  function push(arr, a){ if (arr.length < 400) arr.push([].join.call(a, ' ')); }
  console.warn = function(){ push(window.__CWARN, arguments); return cw.apply(console, arguments); };
  console.error = function(){ push(window.__CERR, arguments); return ce.apply(console, arguments); };
  console.log = function(){ push(window.__CLOG, arguments); return cl.apply(console, arguments); };
  // macroquad 画完就释放绘制缓冲，readPixels 会读到空；测试里强行开 preserveDrawingBuffer。
  var gc = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function(t, a){
    if (t === 'webgl' || t === 'webgl2') { a = a || {}; a.preserveDrawingBuffer = true; }
    return gc.call(this, t, a);
  };
  if (window.__SIM_LOAD_FAIL && window.WebAssembly) {
    // 胶水的 load 是"函数声明"不是"赋值"，defineProperty 截不住（实测照样 boot）。
    // 直接把 WebAssembly 那四个入口换成"必然失败的 promise"：胶水自己 .catch(e => console.error(e))，
    // 正好就是我们要复现的那幕 —— 页面上一个字都不说，只在 console 里留一条。
    var boom = function () { return Promise.reject(new Error('模拟胶水失败：wasm 起不来')); };
    try {
      window.WebAssembly.compile = boom;
      window.WebAssembly.compileStreaming = boom;
      window.WebAssembly.instantiate = boom;
      window.WebAssembly.instantiateStreaming = boom;
    } catch (e) {}
  }
  // 把加载文案的每次变化记下来，用来验「已下载」不会比「总大小」还大。
  function watch(){
    var st = document.getElementById('load-state'), by = document.getElementById('load-bytes');
    if (!st || !by) { setTimeout(watch, 60); return; }
    function snap(){ if (window.__HINTS.length < 300) window.__HINTS.push((st.textContent || '') + ' @ ' + (by.textContent || '')); }
    var mo = new MutationObserver(snap);
    mo.observe(st, {childList: true, subtree: true, characterData: true});
    mo.observe(by, {childList: true, subtree: true, characterData: true});
    snap();
  }
  watch();
})();
return '装了';
"""
TOUCH_SHIM = r"""
try { Object.defineProperty(Navigator.prototype, 'maxTouchPoints', {get: function(){ return 1; }, configurable: true}); } catch (e) {}
try { Object.defineProperty(Window.prototype, 'ontouchstart', {value: null, writable: true, configurable: true}); } catch (e) {}
"""
TOOL = r"""
window.__T = {
  hud: function(){ return (window.__MCL_HUD || '').split('|'); },
  // 中心 20×20 的和 ÷ 周围 60×60 的和：光照整体变时这个比值基本不动，挖掉一块才会动。
  sig: function(){
    var c = document.getElementById('glcanvas'); if (!c) return {err:'没有 canvas'};
    var gl = c.getContext('webgl2') || c.getContext('webgl'); if (!gl) return {err:'没有 GL 上下文'};
    var W = gl.drawingBufferWidth, H = gl.drawingBufferHeight;
    if (W < 64 || H < 64) return {err:'画布太小 ' + W + 'x' + H};
    var big = new Uint8Array(60*60*4), small = new Uint8Array(20*20*4);
    gl.readPixels(((W-60)/2)|0, ((H-60)/2)|0, 60, 60, gl.RGBA, gl.UNSIGNED_BYTE, big);
    var sb = 0, i; for (i = 0; i < big.length; i += 4) sb += big[i]*3 + big[i+1] + big[i+2];
    var x0 = ((W-20)/2)|0, y0 = ((H-20)/2)|0;
    try { gl.readPixels(x0, y0, 20, 20, gl.RGBA, gl.UNSIGNED_BYTE, small); } catch (e) { return {err:'readPixels: ' + e}; }
    var ss = 0; for (i = 0; i < small.length; i += 4) ss += small[i]*3 + small[i+1] + small[i+2];
    return {cen: ss, all: sb, ratio: sb ? ss/sb : 0, w: W, h: H};
  },
  pos: function(){
    var p = (window.__MCL_HUD || '').split('|')[8] || '';
    var m = p.match(/x(-?\d+\.?\d*) y(-?\d+\.?\d*) z(-?\d+\.?\d*)/);
    return m ? [parseFloat(m[1]), parseFloat(m[2]), parseFloat(m[3])] : null;
  },
  pose: function(){
    var p = String((window.__MCL_HUD || '').split('|')[8] || '');
    var m = p.match(/x(-?[0-9.]+) y(-?[0-9.]+) z(-?[0-9.]+) yaw(-?[0-9.]+) pitch(-?[0-9.]+)/);
    return m ? [parseFloat(m[1]), parseFloat(m[2]), parseFloat(m[3]), parseFloat(m[4]), parseFloat(m[5])] : null;
  },
  size: function(){ return [window.innerWidth, window.innerHeight]; },
  // 合成鼠标事件：macroquad 的输入挂在 canvas 上，所以直接派发到 canvas。
  // 同时补一份 pointer*：有些胶水只听 pointer，只听 mouse 也会漏。
  mouse: function(type, x, y, btn){
    var c = document.getElementById('glcanvas');
    var mk = function(kind){
      var cls = kind === 'pointer' ? window.PointerEvent : window.MouseEvent;
      if (!cls) return;
      var opt = {bubbles: true, cancelable: true, clientX: x, clientY: y, button: btn || 0, buttons: btn || 0, view: window};
      if (kind === 'pointer') { opt.pointerId = 1; opt.pointerType = 'mouse'; opt.isPrimary = true; opt.buttons = btn || 0; }
      c.dispatchEvent(new cls((kind === 'pointer' ? 'pointer' : 'mouse') + type.slice(5), opt));
    };
    mk('mouse'); mk('pointer');
  },
  clickAt: function(x, y, btn){
    var c = document.getElementById('glcanvas');
    btn = btn || 0;
    if (btn === 2) c.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, cancelable: true, clientX: x, clientY: y, button: 2, view: window}));
    this.mouse('mousedown', x, y, btn);
    this.mouse('mouseup', x, y, btn);
    c.dispatchEvent(new MouseEvent('click', {bubbles: true, cancelable: true, clientX: x, clientY: y, button: btn, view: window}));
  },
  press: function(x, y, btn){ this.mouse('mousedown', x, y, btn || 0); },
  release: function(x, y, btn){ this.mouse('mouseup', x, y, btn || 0); },
  moveTo: function(x, y){ this.mouse('mousemove', x, y, 0); },
  // 自由转向：不按键挪光标（页面看 ev.buttons，非 0 就不推视角）
  free: function(x, y){ this.mouse('mousemove', x, y, 0); },
  holdMove: function(x, y){ this.mouse('mousemove', x, y, 1); },
  keydown: function(code, key){
    var c = document.getElementById('glcanvas');
    c.dispatchEvent(new KeyboardEvent('keydown', {bubbles: true, code: code, key: key}));
  },
  keyup: function(code, key){
    var c = document.getElementById('glcanvas');
    c.dispatchEvent(new KeyboardEvent('keyup', {bubbles: true, code: code, key: key}));
  },
  tstart: function(id, x, y){ return this.touch('touchstart', [[id, x, y]]); },
  tmove: function(id, x, y){ return this.touch('touchmove', [[id, x, y]]); },
  tend: function(id, x, y){ return this.touch('touchend', [[id, x, y]]); },
  key: function(code, key){
    var c = document.getElementById('glcanvas');
    c.dispatchEvent(new KeyboardEvent('keydown', {bubbles: true, code: code, key: key}));
    c.dispatchEvent(new KeyboardEvent('keyup', {bubbles: true, code: code, key: key}));
  },
  touch: function(type, pts){
    var c = document.getElementById('glcanvas');
    var T = window.TouchEvent, Touch = window.Touch;
    if (!T || !Touch) return '没有 TouchEvent/Touch 构造器';
    var mk = function(list){ return list.map(function(p){ return new Touch({identifier: p[0], target: c, clientX: p[1], clientY: p[2], pageX: p[1], pageY: p[2]}); }); };
    var ended = mk(pts), rest = type === 'touchend' ? [] : ended;
    var e = new T(type, {bubbles: true, cancelable: true, view: window, touches: rest, targetTouches: rest, changedTouches: ended});
    c.dispatchEvent(e);
    return '';
  },
  tap: function(x, y, id){
    var e = this.touch('touchstart', [[id, x, y]]); if (e) return e;
    this.touch('touchend', [[id, x, y]]); return '';
  },
  drag: function(id, x1, y1, x2, y2, steps, pauseMs){
    var e = this.touch('touchstart', [[id, x1, y1]]); if (e) return e;
    var i, t0 = Date.now();
    while (Date.now() - t0 < (pauseMs || 0)) {}
    for (i = 1; i <= steps; i++) {
      this.touch('touchmove', [[id, x1 + (x2-x1)*i/steps, y1 + (y2-y1)*i/steps]]);
    }
    this.touch('touchend', [[id, x2, y2]]); return '';
  },
  hold: function(id, x, y, ms){
    var e = this.touch('touchstart', [[id, x, y]]); if (e) return e;
    var t0 = Date.now(); while (Date.now() - t0 < ms) {}
    this.touch('touchmove', [[id, x, y]]);
    this.touch('touchend', [[id, x, y]]); return '';
  },
  hint: function(){ return (document.getElementById('load-hint') || {}).textContent || ''; },
  why: function(){ return (document.getElementById('fallback-why') || {}).textContent || ''; },
  vis: function(id){ var e = document.getElementById(id); if (!e) return 'noEl'; if (e.hidden) return 'hidden'; var el = e; while (el && el !== document.documentElement) { var cs = getComputedStyle(el); if (cs.display === 'none' || cs.visibility === 'hidden') return 'hidden'; el = el.parentElement; } var rs = e.getClientRects(); if (!rs.length) return 'zero'; var r = rs[0]; return (r.width > 0 && r.height > 0) ? 'shown' : 'zero'; },
  errs: function(){ return (window.__MCL_ERR || []).concat(window.__MCL_RES || []); },
  warns: function(){
    var all = (window.__CWARN || []).concat(window.__CERR || []).concat(window.__CLOG || []);
    return all.filter(function(s){ return /clamping|exceeded max drawcall/i.test(s); }).length;
  }
};
"""


def wait_boot(s, secs=45, autoplay=True):
    t0 = time.time()
    while time.time() - t0 < secs:
        if s.ev("return window.__MCL_BOOTED ? 1 : 0;"):
            return True
        time.sleep(0.4)
    return False


def hud_field(s, i):
    v = s.ev("return window.__T.hud()[%d] || '';" % i)
    return v


def changed_count(s):
    """HUD 上「改动」那一格的数字；读不到就当 0。"""
    try:
        return int(hud_field(s, 4))
    except BaseException:
        return 0


def wait_hud(s, secs=20):
    t0 = time.time()
    while time.time() - t0 < secs:
        if s.ev("return (window.__MCL_HUD || '').split('|').length >= 8 ? 1 : 0;"):
            return True
        time.sleep(0.4)
    return False


def drift_baseline(s, secs=2.0):
    """什么都不做，量一次签名漂移，作为"画面自己会变"的本底。
       返回 (比值漂移, 中心像素和漂移, 全画面像素和的相对漂移)。"""
    a = s.ev("return window.__T.sig();")
    time.sleep(secs)
    b = s.ev("return window.__T.sig();")
    if "err" in a or "err" in b:
        return None, None, None
    rel = abs(b["all"] - a["all"]) / max(abs(b["all"]), 1)
    return (abs(b["ratio"] - a["ratio"]) or 1e-9), (abs(b["cen"] - a["cen"]) or 0), rel


def rel_diff(a, b):
    """全画面像素和的相对差：跨会话比"是不是同一个世界"用这个。"""
    return abs(a - b) / max(abs(b), 1)


def m_press(s, x, y, btn=0):
    s.ev("window.__T.press(%d, %d, %d);" % (x, y, btn))


def m_release(s, x, y, btn=0):
    s.ev("window.__T.release(%d, %d, %d);" % (x, y, btn))


def m_move(s, x, y):
    s.ev("window.__T.moveTo(%d, %d);" % (x, y))


def click(s, x, y, btn=0, hold=0.35, after=0.7):
    """按下、真的等几帧、再松手。

    macroquad 的判据是跨帧的：按下要被某一帧看到，松手要在之后的某一帧，然后才算一次点击。
    以前我把 down/up/click 在同一拍里连发，鼠标路径整套逻辑一次都没被触发，
    于是"挖不动"被我误读成"画面不重画"——白查半天。
    """
    m_press(s, x, y, btn)
    time.sleep(hold)
    m_release(s, x, y, btn)
    time.sleep(after)


CUR = [0, 0]          # 我们自己盯着光标在哪：画布就这么大，撞到边就得挪光标


def free_move(s, x, y):
    """不按键把光标挪到 (x,y)：现在鼠标一动页面就往 wasm 推视角。"""
    s.ev("window.__T.free(%d, %d);" % (x, y))
    CUR[0], CUR[1] = x, y
    time.sleep(0.12)


def hop_cursor(s, tx, ty):
    """按住**中键**把光标挪走：按着的时候页面不推视角（不转），而中键松手在 Rust 那边是
       「把准星指着的那种方块换到手里」，不会改地图，所以不会污染「改动」计数。"""
    s.ev("window.__T.press(%d, %d, 1);" % (CUR[0], CUR[1]))
    time.sleep(0.1)
    s.ev("window.__T.holdMove(%d, %d);" % (tx, ty))
    time.sleep(0.1)
    s.ev("window.__T.release(%d, %d, 1);" % (tx, ty))
    time.sleep(0.45)
    CUR[0], CUR[1] = tx, ty


def drag_look(s, x, y, dy, steps=6, per=0.12):
    """把视角转 dy 像素对应的角度：不按键挪光标（鼠标一动就转）。
       光标撞到画布上下边就用 hop_cursor 拉回中线接着走，累计角度不受窗口大小限制。"""
    h = s.ev("return window.innerHeight;")
    if CUR[0] == 0 and CUR[1] == 0:
        CUR[0], CUR[1] = x, y
    remain = dy
    for _ in range(14):
        if abs(remain) <= 1:
            break
        step = 260 if remain > 0 else -260
        ty = max(24, min(h - 24, CUR[1] + step))
        if abs(ty - CUR[1]) < 3:              # 撞到画布边了：把光标挪回中间再继续
            hop_cursor(s, x, h // 2)
            continue
        free_move(s, CUR[0], ty)
        remain -= (ty - CUR[1])
        time.sleep(per)
    return dy


def pose(s):
    p = s.ev("return window.__T.pose();")
    return None if not p else [float(v) for v in p]


def ang(a):
    """把角度折到 (-pi, pi]：yaw 会绕圈，直接减会算出 2π 的假变化。"""
    import math
    a = float(a)
    while a > math.pi:
        a -= 2 * math.pi
    while a <= -math.pi:
        a += 2 * math.pi
    return a


def dig_with_aim(s, W, H, initial_dy=0):
    """挖一下试试：出生时是平视，准星可能只对着天空（6.5 格内没目标就挖不动），
       那就压一次视角再试。返回 (累计视角位移, 改动增量, 只跨这一次点击的比值差)。
       累计位移后面要在每个新会话里原样重演一遍，跨会话的画面比值才可比。"""
    total = initial_dy
    for dy in [0] + [d for d in (110, -110, 200, -200, 300, -300)]:
        if dy:
            drag_look(s, W // 2, H // 2, dy)
            total += dy
        n0 = changed_count(s)
        a = s.ev("return window.__T.sig();")
        click(s, W // 2, H // 2, 0)
        n1 = changed_count(s)
        b = s.ev("return window.__T.sig();")
        if n1 > n0 and "err" not in a and "err" not in b:
            return total, n1 - n0, (b["cen"] - a["cen"]), abs(b["ratio"] - a["ratio"])
    return total, 0, 0, 0.0


def aligned_sig(s, t_boot, offset=8.0):
    """在"开跑之后固定秒数"取画面签名：昼夜 360 秒一轮，采样时刻对齐了跨会话才可比。"""
    wait = (t_boot + offset) - time.time()
    if wait > 0:
        time.sleep(wait)
    return s.ev("return window.__T.sig();")


DB_JS = """
fetch('loader.js').then(function (r) { return r.text(); }).then(function (src) {
  function grab(n) {
    var m = new RegExp('var\\\\s+' + n + '\\\\s*=\\\\s*(?:\\'([^\\']*)\\'|"([^"]*)"|(\\\\d+))').exec(src);
    return m ? (m[1] || m[2] || m[3]) : null;
  }
  var DBN = grab('DB_NAME'), ST = grab('STORE'), K = grab('KEY');
  if (!DBN || !ST || !K) { window.__DBS = {err: '读不出 DB_NAME/STORE/KEY'}; return; }
  var out = {};
  try {
    var rq = indexedDB.open(DBN);
    rq.onerror = function () { window.__DBS = {err: 'open 失败'}; };
    rq.onsuccess = function () {
      var db = rq.result;
      if (!db.objectStoreNames.contains(ST)) { db.close(); window.__DBS = {saved: 'store 不存在'}; return; }
      var g = db.transaction(ST, 'readonly').objectStore(ST).get(K);
      g.onsuccess = function () {
        var v = g.result;
        if (v == null) out.saved = null;
        else if (typeof v === 'string') out.saved = {kind: 'string', len: v.length};
        else out.saved = {kind: 'bytes', len: new Uint8Array(v).length};
        db.close(); window.__DBS = out;
      };
      g.onerror = function () { db.close(); window.__DBS = {err: '读失败'}; };
    };
  } catch (e) { window.__DBS = {err: 'indexedDB 用不了: ' + e}; }
}, function (e) { window.__DBS = {err: 'fetch loader.js 失败: ' + e}; });
"""


def db_state(s, timeout=8):
    """直接去 IndexedDB 看一眼存档到底在不在（别信 HUD 那张嘴）。"""
    s.ev("window.__DBS = null;")
    s.ev(DB_JS)
    t0 = time.time()
    while time.time() - t0 < timeout:
        v = s.ev("return window.__DBS;")
        if v is not None:
            return v
        time.sleep(0.2)
    return {"err": "超时"}


def fresh_boot(s, u, total_dy):
    """重开一个全新会话，并原样重演那次视角位移，让它和上一个会话可比。"""
    s.go(u + "/game/index.html?autoplay=1")
    s.ev(TOOL)
    if not wait_boot(s):
        return None, 0
    t_boot = time.time()
    wait_hud(s)
    W, H = s.ev("return window.__T.size();")
    if total_dy:
        drag_look(s, W // 2, H // 2, total_dy)
    return t_boot, max(W, 0)


def scene_base(u):
    s = Sess(hook=HOOK)
    s.go(u + "/game/index.html?autoplay=1")
    s.ev(TOOL)
    ok_boot = wait_boot(s)
    t_boot = time.time()
    wait_hud(s)
    time.sleep(2.0)
    W, H = s.ev("return window.__T.size();")
    log("桌面基础", "跑起来了", ok_boot, "boot=%s 窗口 %dx%d" % (ok_boot, W, H))
    if not ok_boot:
        return
    gate_gone = False
    for _ in range(20):
        if s.ev("return window.__T.vis('gate');") == "hidden":
            gate_gone = True
            break
        time.sleep(0.25)
    log("桌面基础", "门面收掉了", gate_gone, "跑起来 %s 秒后门面=%s"
        % (round(time.time() - t_boot, 1), s.ev("return window.__T.vis('gate');")))
    log("桌面基础", "兜底没露出", s.ev("return window.__T.vis('fallback');") == "hidden")
    hud = s.ev("return window.__MCL_HUD;")
    log("桌面基础", "HUD 有 9 段（第 9 段是给我的调试位姿）", len(hud.split("|")) == 9, hud)
    dom = s.ev("return ['hud-slot','hud-block','hud-mode','hud-clock','hud-changed','hud-fps','hud-save','hud-note']"
               ".map(function(i){ var e=document.getElementById(i); return e ? e.textContent : '(没有这个元素)'; });")
    parts = hud.split("|")
    names = ["槽位", "方块", "模式", "时刻", "改动", "帧率", "存档", "备注"]
    diff = [(nm, "wasm=%r" % w, "页面=%r" % d) for nm, w, d in zip(names, parts[:8], dom)
            if nm != "时刻" and d != w and not (w == "" and d == "")]
    log("桌面基础", "HUD 其余七格真的写进了页面（不是只有 wasm 在推）", len(diff) == 0,
        ("对不上的：%s" % diff if diff else "七格全对得上"))
    c1 = hud_field(s, 3)
    time.sleep(1.2)
    pair = s.ev("return [String((window.__MCL_HUD || '').split('|')[3] || ''), "
                "(document.getElementById('hud-clock') || {}).textContent || ''];")
    c2, dom_clock = (list(pair) + ["", ""])[:2]
    def mins(t):
        try:
            a, b = str(t).split(":")
            return int(a) * 60 + int(b)
        except Exception:
            return -1
    gap = mins(c2) - mins(dom_clock)
    log("桌面基础", "HUD 的时刻那一格在动，页面写的和 wasm 推的对得上（最多差一分钟）",
        c1 != c2 and 0 <= gap <= 1,
        "%s → %s；同一瞬间 wasm=%s 页面=%s（差 %d 分钟：wasm 每 0.2 秒推一次，"
        "页面每 200ms 才写一次 DOM，而游戏里一分钟只要约 0.25 秒真时间，落后一分钟是正常的）" % (
            c1, c2, c2, dom_clock, gap))
    log("桌面基础", "HUD 本体和准星都显出来了",
        s.ev("return window.__T.vis('hud');") == "shown" and s.ev("return window.__T.vis('cross');") == "shown")
    log("桌面基础", "调试位姿解得出来", s.ev("return window.__T.pos() !== null ? 1 : 0;") == 1,
        json.dumps(s.ev("return window.__T.pos();")))
    errs = s.ev("return window.__T.errs();")
    log("桌面基础", "页面没有报错/资源 404", not errs, "｜".join(errs[:3])[:160])
    warn = s.ev("return window.__T.warns();")
    log("桌面基础", "没有 drawcall 截断警告（旧版实测每帧几十条）", warn == 0, "累计 %d 条" % warn)
    used = s.ev("return performance.getEntriesByType('resource').map(function(e){return e.name;})"
                ".filter(function(n){return /wasm/i.test(n);});")
    log("桌面基础", "用的是带哈希的 wasm 地址",
        any("mc-lite." in x and not x.endswith("mc-lite.wasm") for x in used), str(used)[:150])
    hints = s.ev("return window.__HINTS;") or []
    kb = re.compile(r"([\d.]+)\s*KB\s*/\s*([\d.]+)\s*KB")
    bad = [h for h in hints if kb.search(h) and float(kb.search(h).group(1)) > float(kb.search(h).group(2)) + 0.05]
    if hints:
        log("桌面基础", "进度条不会「已下载 > 总大小」", not bad, (bad[0][:80] if bad else "抓到 %d 条文案" % len(hints)))
    else:
        log("桌面基础", "加载文案没抓到样本（本机下载太快，不作数）", None, "样本 0 条")
    sig0 = s.ev("return window.__T.sig();")
    log("桌面基础", "读得到画布像素（preserveDrawingBuffer 钩子生效）",
        "err" not in sig0 and sig0.get("cen", 0) > 0, "中心和 %s / 全画面和 %s" % (sig0.get("cen"), sig0.get("all")))
    if "err" in sig0:
        return
    base, base_cen, base_all = drift_baseline(s)
    base = base or 1e-9
    CEN = max(12 * (base_cen or 0), 4000)
    ALLTOL = max(12 * (base_all or 0), 0.004)
    log("桌面基础", "什么都不做时的画面漂移已测", True,
        "中心像素和漂移 %s／2 秒（判重画要 > %d）；全画面相对漂移 %.5f／2 秒（跨会话判同一个世界容差取 %.4f）"
        % (base_cen, CEN, base_all, ALLTOL))

    # ── 存档这条先做：这时候谁都没碰过视角，两个会话看到的画面才一模一样，比值才可比 ──
    s0 = aligned_sig(s, t_boot, 8.0)
    n0 = changed_count(s)
    click(s, W // 2, H // 2, 0)
    n1 = changed_count(s)
    sd = s.ev("return window.__T.sig();")
    dug0 = n1 > n0
    d_cen = (sd["cen"] - s0["cen"]) if "err" not in sd else 0
    log("桌面基础", "点一下能挖动", dug0, "出生视角下直接挖，改动 %d → %d" % (n0, n1))
    log("桌面基础", "挖一下之后画面真的重画了（旧版完全不重画）", dug0 and d_cen > CEN,
        "这一刀让中心像素和变了 %+d（阈值 %d）" % (d_cen, CEN))
    time.sleep(4.5)                        # 自动存档 3 秒一轮，等它写一次
    saved = changed_count(s)
    tb2, _ = fresh_boot(s, u, 0)
    s1 = aligned_sig(s, tb2) if tb2 else {"err": "刷新后没起来"}
    gap1 = rel_diff(s1.get("all", 0), s0.get("all", 0)) if "err" not in s1 else 9.0
    log("存档", "刷新之后挖过的坑还在（真读到了 IndexedDB 那份）", dug0 and gap1 > ALLTOL,
        "刷新后 vs 挖之前：全画面差 %.4f（容差 %.4f，共 %d 格改动）" % (gap1, ALLTOL, saved))
    if "err" not in s1:
        log("存档", "刷新前那一刻的改动数也带过来了", changed_count(s) >= saved,
            "改动 %d → %d" % (saved, changed_count(s)))
    s.ev("document.getElementById('btn-reset').click();")
    time.sleep(2.5)                        # 清档那条写存储是异步的，先等它落地
    dbs = db_state(s)
    log("清档", "清档真的把 IndexedDB 那份删掉了", isinstance(dbs, dict) and dbs.get("saved") is None,
        json.dumps(dbs, ensure_ascii=False)[:150])
    # 清档之后页面自己会 location.replace 刷新。那是页面发起的导航：我这头装钩子必然慢一步，
    # readPixels 读出来全是 0。所以这里由我用 WebDriver 再走一次地址，让钩子赶在 wasm 之前装上。
    s.go(u + "/game/index.html?autoplay=1")
    s.ev(TOOL)
    if not wait_boot(s, 40):
        wait_boot(s, 20)
    tb3 = time.time()
    wait_hud(s)
    s2 = aligned_sig(s, tb3)
    if "err" in s2:
        log("清档", "清档之后没取到画面", False, str(s2))
    else:
        gap0 = rel_diff(s2["all"], s0["all"])
        log("清档", "清档重开真的回到种子初始岛", gap0 < ALLTOL and gap1 > ALLTOL,
            "清档后 vs 挖之前差 %.4f（要小）；刷新后 vs 挖之前差 %.4f（要大）—— 两头都对上才算存档这条通了"
            % (gap0, gap1))
    log("清档", "清档后改动归零", changed_count(s) == 0, "改动=%s" % changed_count(s))
    errs = s.ev("return window.__T.errs();")
    log("清档", "整条存档往返没有报错", not errs, "｜".join(errs[:3])[:160])

    # ── 交互：接下来要动视角了，这些用例只在同一个会话里比 ──
    if not wait_boot(s, 5):
        log("桌面基础", "清档之后没能继续在跑，交互用例跳过", False)
        return
    p0 = s.ev("return window.__T.pos();")
    s.ev("window.__T.keydown('KeyW','w');")
    time.sleep(2.0)
    s.ev("window.__T.keyup('KeyW','w');")
    time.sleep(0.4)
    p1 = s.ev("return window.__T.pos();")
    moved = (abs(p1[0] - p0[0]) + abs(p1[2] - p0[2])) if p0 and p1 else 0
    log("桌面基础", "按住 W 真的会前进（焦点已还给画布）", bool(moved > 0.5),
        "位移 %.2f（%s → %s）" % (moved, p0, p1))

    # --- 方向类判据。公式和 phys.rs 里两条单元测试同源：
    #     屏幕右 = cross(视线, 上) = (-cos yaw, 0, sin yaw)；yaw 增大是往**左**转
    #     （那边拿渲染用的 Mat4::look_at_rh 实测过：yaw+0.3 之后原来正前方那点跑到画面右边 x=+2.96）。
    def hold_key(code, ch, secs):
        s.ev("window.__T.keydown('%s','%s');" % (code, ch))
        time.sleep(secs)
        s.ev("window.__T.keyup('%s','%s');" % (code, ch))
        time.sleep(0.5)

    def basis(p):
        import math
        return ([math.sin(p[3]), math.cos(p[3])], [-math.cos(p[3]), math.sin(p[3])])  # (前向, 屏幕右) 的 (x,z)

    for key, ch, wantf, wantr, label in [
        ("KeyS", "s", -1, 0, "按 S 是往后退"),
        ("KeyD", "d", 0, 1, "按 D 是往屏幕右边平移（不是转向）"),
        ("KeyA", "a", 0, -1, "按 A 是往屏幕左边平移（不是转向）"),
    ]:
        p0 = pose(s)
        hold_key(key, ch, 1.5)
        p1 = pose(s)
        if not p0 or not p1:
            log("桌面基础", label, None, "读不到位姿")
            continue
        f, r = basis(p0)
        dx, dz = p1[0] - p0[0], p1[2] - p0[2]
        along = dx * f[0] + dz * f[1]
        side = dx * r[0] + dz * r[1]
        dyaw = ang(p1[3] - p0[3])
        ok = abs(dyaw) < 0.03
        ok = ok and ((along * wantf > 0.5) if wantf else (abs(along) < 1.0))  # 视线那一维
        ok = ok and ((side * wantr > 0.5) if wantr else (abs(side) < 1.0))    # 屏幕右那一维
        log("桌面基础", label, ok,
            "位移 (%.2f, %.2f)：沿视线 %.2f、沿屏幕右 %.2f（要 %s 那个方向为正），yaw 变了 %.4f" % (
                dx, dz, along, side, "视线" if wantf else "屏幕右", dyaw))

    # 鼠标一动就转，而且转的方向和鼠标移动同向：往右移光标 = 往右转 = yaw 要减
    cx, cy = W // 2, H // 2
    free_move(s, cx, cy)
    free_move(s, cx, cy)                    # 头一拍只记起点，不产生转向
    p0 = pose(s)
    for kk in range(1, 9):
        free_move(s, cx + 16 * kk, cy)
    time.sleep(0.8)
    p1 = pose(s)
    want = -(16 * 8) * math.pi / 700.0
    d_yaw = ang(p1[3] - p0[3]) if p0 and p1 else 0.0
    log("桌面基础", "鼠标一动视角就转，而且往鼠标动的方向转",
        bool(p0 and p1) and d_yaw < -0.1 and abs(d_yaw - want) < abs(want) * 0.6,
        "光标右移 128 像素：yaw %.3f → %.3f（变了 %.3f，按 π/700 该是 %.3f：往右转 yaw 要减）" % (
            p0[3], p1[3], d_yaw, want) if p0 and p1 else "读不到位姿")
    for kk in range(7, -1, -1):
        free_move(s, cx + 16 * kk, cy)
    time.sleep(0.8)
    p2 = pose(s)
    log("桌面基础", "把鼠标挪回原地视角也转回去",
        bool(p0 and p2) and abs(ang(p2[3] - p0[3])) < 0.12,
        "转回去之后 yaw 还差 %.3f" % ang(p2[3] - p0[3]) if p0 and p2 else "")
    free_move(s, cx, cy)
    free_move(s, cx, cy)

    drag_look(s, W // 2, H // 2, 420)       # 压向地面：平视挖掉一格之后准星就只对着天空了
    total_dy, dug, d_cen2, d_ratio = dig_with_aim(s, W, H, initial_dy=420)
    for _ in range(2):
        n0 = changed_count(s)
        click(s, W // 2, H // 2, 0, hold=0.3, after=0.4)
        if changed_count(s) > n0:
            dug += 1
    log("桌面基础", "压视角后连挖三下都真挖动（旧版挖一刀就不重画）", dug >= 3,
        "视角压到 %d 像素，连挖真挖动 %d 下，最后一刀中心像素和 %+d" % (total_dy, dug, d_cen2))
    # 就在刚挖出目标的那个俯角上放：这时候准星一定还有目标（上一刀刚挖中过），
    # 要放的格子就是刚挖出来的那个洞。仰着打就没目标了，会把"没目标"误判成"放不下去"。
    placed_at, n0, n1 = None, changed_count(s), 0
    for dy in (0, -120, -240, -360, -480, 180):
        if placed_at is not None:
            break
        if dy:
            drag_look(s, W // 2, H // 2, dy)
            total_dy += dy
        n0 = changed_count(s)
        click(s, W // 2, H // 2, 2)
        n1 = changed_count(s)
        if n1 > n0:
            placed_at = dy
    log("桌面基础", "右键能放方块", placed_at is not None,
        "改动 %d → %d（在比原来%s的俯角上放下去了）" % (n0, n1,
        "抬 %d 像素" % (-placed_at) if placed_at and placed_at < 0 else ("压 %d 像素" % placed_at if placed_at else "一样的角度")))
    if placed_at is None:
        log("桌面基础", "换了 6 个俯角都放不下去（那就真是 bug）", False, "最后一处改动 %d → %d" % (n0, n1))

    s.ev("window.__T.keydown('KeyF','f');")
    time.sleep(0.4)
    s.ev("window.__T.keyup('KeyF','f');")
    time.sleep(0.6)
    mode = hud_field(s, 2)
    note = s.ev("return (document.getElementById('hud-note')||{}).textContent || '';")
    log("桌面基础", "按 F 能飞（HUD 模式=飞行）", "飞行" in mode, "模式=%s" % mode)
    log("桌面基础", "HUD 有 #hud-note 并显示第 8 段（旧版第 8 段没人显示）", "飞行" in note,
        "文案=%r" % note[:60])
    p0 = s.ev("return window.__T.pos();")
    s.ev("window.__T.keydown('Space',' ');")
    time.sleep(0.7)                          # 跳跃看的是"这一帧空格按着没"，同一拍里按下又抬起等于没按
    s.ev("window.__T.keyup('Space',' ');")
    time.sleep(0.35)
    p1 = s.ev("return window.__T.pos();")
    log("桌面基础", "按空格会跳（y 变大）", bool(p0 and p1 and p1[1] > p0[1] + 0.05) if p0 and p1 else None,
        "y %.2f → %.2f" % (p0[1], p1[1]) if p0 and p1 else "读不到位姿（第 9 段没有）")
    s.ev("window.__T.keydown('KeyF','f');")
    time.sleep(0.4)
    s.ev("window.__T.keyup('KeyF','f');")
    time.sleep(0.6)
    log("桌面基础", "再按 F 关飞行", "飞行" not in hud_field(s, 2), "模式=%s" % hud_field(s, 2))

    m_press(s, W // 2, H // 2)               # 在画布外松手：胶水只给画布绑了松手事件。跑着的时候在顶栏、页脚松的手，wasm 那边永远
    # 看不到松手，鼠标键状态卡在「还按着」——而按着的时候页面是不推视角的，所以卡住的直接
    # 表现变成「之后鼠标再怎么动都不转」（旧版没有自由转向，那会儿的表现是一动就自转）。
    time.sleep(0.35)
    p0 = pose(s)
    s.ev("window.__T.press(%d, %d, 0);" % (W // 2, H // 2))
    time.sleep(0.15)
    s.ev("window.__T.holdMove(%d, 8);" % (W // 2))
    time.sleep(0.15)
    s.ev("var e=new MouseEvent('mouseup',{bubbles:true,clientX:%d,clientY:8,button:0,buttons:0});"
         "document.body.dispatchEvent(e);"
         "var pe=new PointerEvent('pointerup',{bubbles:true,clientX:%d,clientY:8,button:0,buttons:0,"
         "pointerId:1,pointerType:'mouse',isPrimary:true});document.body.dispatchEvent(pe);"
         % (W // 2, W // 2))
    time.sleep(0.35)
    CUR[0], CUR[1] = W // 2, 8
    free_move(s, W // 2, H // 2)            # 回到画布里：这一拍只用来记起点
    for kk in range(1, 9):
        free_move(s, W // 2 + 16 * kk, H // 2)
    time.sleep(0.8)
    p1 = pose(s)
    d_yaw = ang(p1[3] - p0[3]) if p0 and p1 else 0.0
    log("桌面基础", "在画布外松手不会把鼠标键卡住（卡住的话之后鼠标再怎么动都不转）",
        abs(d_yaw) > 0.15, "画布外松手后再自由挪 128 像素：yaw 又动了 %.3f" % d_yaw)
    errs = s.ev("return window.__T.errs();")
    log("桌面基础", "这一轮交互没有报错", not errs, "｜".join(errs[:3])[:160])


def scene_nowebgl(u):
    s = Sess(prefs={"webgl.disabled": True}, hook=HOOK)
    s.go(u + "/game/index.html?autoplay=1")
    s.ev(TOOL)
    t0 = time.time()
    why = ""
    while time.time() - t0 < 30:
        why = s.ev("return window.__T.why();")
        if why:
            break
        time.sleep(0.5)
    vis = s.ev("return window.__T.vis('fallback');")
    log("没有 WebGL", "给的是中文原因而不是英文弹窗", vis == "shown" and "WebGL" in why, "原因=%r" % why[:120])
    log("没有 WebGL", "loader 确实跑了（不是白屏）", s.ev("return window.__MCL_LOADER_RAN ? 1 : 0;") == 1)
    btn = s.ev("return (document.getElementById('btn-play')||{}).textContent || '';")
    log("没有 WebGL", "按钮变成可点的「刷新重试」", "重试" in btn, "按钮文案=%r" % btn[:40])
    s.close()


def scene_watchdog(u):
    s = Sess(hook="window.__SIM_LOAD_FAIL = 1;" + HOOK)   # 标志要在钩子那一段之前置好
    s.go(u + "/game/index.html?autoplay=1")
    s.ev(TOOL)
    time.sleep(3.0)
    armed = s.ev("return window.__MCL_BOOTED ? 0 : 1;")
    if not armed:
        log("胶水吞错", "模拟没生效（wasm 还是起来了），这条不算数", None, "boot 已经置位 = 模拟失败")
        s.close()
        return
    t0 = time.time()
    why = ""
    while time.time() - t0 < 45:
        why = s.ev("return window.__T.why();")
        if why:
            break
        time.sleep(1.0)
    vis = s.ev("return window.__T.vis('fallback');")
    log("胶水吞错", "wasm 起不来时会写明原因（旧版永远卡在「编译 wasm」）",
        vis == "shown" and "模拟" in why, "%.0f 秒后原因=%r" % (time.time() - t0, why[:120]))
    btn = s.ev("return (document.getElementById('btn-play')||{}).textContent || '';")
    disabled = s.ev("return document.getElementById('btn-play').disabled ? 1 : 0;")
    log("胶水吞错", "按钮没留在灰的「加载中」", "重试" in btn and disabled == 0, "文案=%r disabled=%s" % (btn[:30], disabled))
    s.close()


def scene_gate(u):
    s = Sess(w=380, h=560, hook=HOOK)
    s.go(u + "/game/index.html")                              # 不加 autoplay：真人第一屏
    s.ev(TOOL)
    s.ev("var g=document.getElementById('gate'); if(g) g.scrollTop = 9999;")
    time.sleep(0.4)
    d = s.ev("var g=document.getElementById('gate'); var k=document.querySelector('.g-kicker')||document.querySelector('.g-gate-in');"
             "return [g.scrollHeight, g.clientHeight, g.scrollTop, k ? Math.round(k.getBoundingClientRect().top - g.getBoundingClientRect().top) : -999,"
             " getComputedStyle(g).touchAction, g.hidden ? 'hidden' : 'shown'];")
    sh, ch, top, ktop, tact, vis = d
    scrollable = sh > ch
    s.ev("var g=document.getElementById('gate'); if(g) g.scrollTop = 0;")
    time.sleep(0.3)
    ktop0 = s.ev("var g=document.getElementById('gate'); var k=document.querySelector('.g-kicker')||document.querySelector('.g-gate-in');"
                 "return k ? Math.round(k.getBoundingClientRect().top - g.getBoundingClientRect().top) : -999;")
    log("小屏门面", "门面在小屏能滚", scrollable, "scrollHeight=%s clientHeight=%s 门面=%s" % (sh, ch, vis))
    log("小屏门面", "滚到顶时顶部内容真的可达（旧版是滚不到的死区）", (not scrollable) or ktop0 >= -2,
        "滚到顶后标题相对门面顶 =%d px（旧版为负且够不着）" % ktop0)
    log("小屏门面", "门面上开了竖向触摸滚动 touch-action:pan-y", "pan-y" in (tact or ""), "touch-action=%s" % tact)
    log("小屏门面", "真人第一屏能看到「点开玩」", s.ev("return window.__T.vis('btn-play');") == "shown")
    s.ev("document.getElementById('btn-play').click();")
    ok = wait_boot(s, 45)
    time.sleep(1.5)
    act = s.ev("return document.activeElement ? (document.activeElement.id || document.activeElement.tagName) : '?';")
    log("小屏门面", "点完「点开玩」焦点回到画布（旧版 WASD 没反应）", act == "glcanvas", "activeElement=%s" % act)
    log("小屏门面", "点按钮这条路也能跑起来", ok)
    s.close()


def scene_touch(u):
    s = Sess(prefs={"dom.w3c_touch_events.enabled": 1}, w=420, h=760, hook=HOOK + TOUCH_SHIM)
    s.go(u + "/game/index.html?autoplay=1")
    s.ev(TOOL)
    if not wait_boot(s):
        log("手机端触摸", "跑起来了", False, "没 boot，后面全跳过")
        return
    wait_hud(s)
    time.sleep(2.0)
    if not s.ev("return (window.TouchEvent && window.Touch) ? 1 : 0;"):
        log("手机端触摸", "这个内核不给 TouchEvent，触摸用例没跑成", False,
            "typeof TouchEvent=%s" % s.ev("return typeof window.TouchEvent;"))
        return
    W, H = s.ev("return window.__T.size();")
    right = [int(W * 0.72), int(H * 0.55)]
    left = [int(W * 0.22), int(H * 0.62)]

    touch_env = s.ev("return (('ontouchstart' in window) || navigator.maxTouchPoints > 0 ||"
                     " (window.matchMedia && window.matchMedia('(any-pointer: coarse)').matches)) ? 1 : 0;")
    # 无头 Firefox 报不出触屏能力：ontouchstart 不在 window 里、maxTouchPoints 是 0、
    # any-pointer 也不是 coarse；用 wrappedJSObject 或者改 Navigator/Window 原型往回塞都塞不进页面作用域（都试过）。
    # 所以"是触屏才显按钮"这一步只能验到布局和桥（下面那几条），显不显这条在这儿只能不作数。
    if not touch_env:
        log("手机端触摸", "这台无头浏览器报不出触屏能力（下面两条只能不作数）", None,
            "ontouch=%s maxTouchPoints=%s" % (s.ev("return ('ontouchstart' in window) ? 1 : 0;"),
                                              s.ev("return navigator.maxTouchPoints;")))
    log("手机端触摸", "操作提示换成了触屏那一套（桌面那行收掉）",
        (None if not touch_env else
         s.ev("return window.__T.vis('hint-desktop');") == "hidden" and s.ev("return window.__T.vis('hint-touch');") == "shown"),
        "桌面那行=%s 触屏那行=%s（ontouch=%s maxTouchPoints=%s）" % (
            s.ev("return window.__T.vis('hint-desktop');"), s.ev("return window.__T.vis('hint-touch');"),
            s.ev("return ('ontouchstart' in window) ? 1 : 0;"), s.ev("return navigator.maxTouchPoints;")))
    log("手机端触摸", "触屏 HUD 按钮出来了（跳/飞/换方块）",
        (None if not touch_env else
         s.ev("return window.__T.vis('btn-jump');") == "shown" and s.ev("return window.__T.vis('btn-fly');") == "shown"),
        "跳=%s 飞=%s 上一个=%s 下一个=%s" % (s.ev("return window.__T.vis('btn-jump');"), s.ev("return window.__T.vis('btn-fly');"),
                                          s.ev("return window.__T.vis('btn-slot-prev');"), s.ev("return window.__T.vis('btn-slot-next');")))

    def tap(tid, x, y, gap=0.14, after=1.0):
        s.ev("window.__T.tstart(%d, %d, %d);" % (tid, x, y))
        time.sleep(gap)                     # 按住时长是靠过帧累计的，同一拍里发完等于 0 秒
        s.ev("window.__T.tend(%d, %d, %d);" % (tid, x, y))
        time.sleep(after)

    # 出生是平视，准星可能只对着天空：先压视角，直到真能挖动
    tid, total = 1, 0
    aimed = False
    for dy in (110, -110, 200, -200, 300, -300):
        n0 = changed_count(s)
        tap(tid, right[0], right[1]); tid += 1
        if changed_count(s) > n0:
            aimed = True
            break
        s.ev("window.__T.tstart(%d, %d, %d);" % (tid, right[0], right[1])); time.sleep(0.12); tid += 1
        for k in range(1, 7):
            s.ev("window.__T.tmove(%d, %d, %d);" % (tid - 1, right[0], right[1] + dy * k // 6))
            time.sleep(0.09)
        s.ev("window.__T.tend(%d, %d, %d);" % (tid - 1, right[0], right[1] + dy))
        time.sleep(0.4)
        total += dy
    def look_drag(px, dy):
        """右半屏按住拖一下转视角（每步之间真的过帧），返回下一个触点 id。"""
        s.ev("window.__T.tstart(%d, %d, %d);" % (px, right[0], right[1]))
        time.sleep(0.12)
        for k in range(1, 7):
            s.ev("window.__T.tmove(%d, %d, %d);" % (px, right[0], right[1] + dy * k // 6))
            time.sleep(0.09)
        s.ev("window.__T.tend(%d, %d, %d);" % (px, right[0], right[1] + dy))
        time.sleep(0.4)
        return px + 1

    # 平视挖掉面前那一格之后，准星就只对着天空了：压一次俯角，给后面这几刀和这次放方块备着目标
    tid = look_drag(tid, 420)
    dug1 = 0
    def half_drag(dx, dy):
        """右半屏拖一下（分步、每步真的过帧）。"""
        s.ev("window.__T.tstart(%d, %d, %d);" % (tid, right[0], right[1]))
        time.sleep(0.12)
        for kk in range(1, 7):
            s.ev("window.__T.tmove(%d, %d, %d);" % (tid, right[0] + dx * kk // 6, right[1] + dy * kk // 6))
            time.sleep(0.09)
        s.ev("window.__T.tend(%d, %d, %d);" % (tid, right[0] + dx, right[1] + dy))
        time.sleep(0.5)
        return tid + 1

    p0 = pose(s)
    tid = half_drag(150, 0)          # 往右拖
    p1 = pose(s)
    tid = half_drag(0, -150)         # 往上拖（先离地：pitch 夹紧在 ±(π/2−0.02)，抵着底再往下拖测不出变化）
    p2 = pose(s)
    tid = half_drag(0, 300)          # 再往下拖
    p3 = pose(s)
    if p0 and p1 and p2 and p3:
        dyaw_r = ang(p1[3] - p0[3])
        dpitch_u = p2[4] - p1[4]
        dpitch_d = p3[4] - p2[4]
        log("手机端触摸", "视角转动方向和手指拖动方向一致",
            dyaw_r < -0.1 and dpitch_u > 0.1 and dpitch_d < -0.1,
            "往右拖 150 像素：yaw %.3f → %.3f（往右转 yaw 该减，变了 %.3f）；"
            "往上拖 150：pitch %.3f → %.3f（该抬，变了 %.3f）；往下拖 300：→ %.3f（该压，变了 %.3f）" % (
                p0[3], p1[3], dyaw_r, p1[4], p2[4], dpitch_u, p3[4], dpitch_d))
    else:
        log("手机端触摸", "视角转动方向和手指拖动方向一致", None, "读不到位姿")

    for _ in range(3):
        n0 = changed_count(s)
        tap(tid, right[0], right[1]); tid += 1
        n1 = changed_count(s)
        if n1 > n0:
            dug1 = n1 - n0
            break
        tid = look_drag(tid, 150)
    log("手机端触摸", "轻点一下只挖一格（旧版触屏还模拟鼠标 → 挖两下）", dug1 == 1,
        "改动 %d → %d%s" % (n0, n1, "" if dug1 else "（试了三回准星都没目标，这条不算数）"))
    if not dug1:
        log("手机端触摸", "手机端准星一直对不到东西", False, "压了 %d 像素视角还挖不动" % total)
        return

    # 按住 0.7 秒：只该放一个方块，不该在抬手时再挖一次
    n0 = changed_count(s)
    s.ev("window.__T.tstart(%d, %d, %d);" % (tid, right[0], right[1])); time.sleep(0.7)
    s.ev("window.__T.tend(%d, %d, %d);" % (tid, right[0], right[1])); tid += 1
    time.sleep(1.1)
    placed = None
    for k, dy in enumerate((0, -120, -240, -360, -480)):
        if k:
            tid = look_drag(tid, dy)
        n0 = changed_count(s)
        s.ev("window.__T.tstart(%d, %d, %d);" % (tid, right[0], right[1])); time.sleep(0.7)
        s.ev("window.__T.tend(%d, %d, %d);" % (tid, right[0], right[1])); tid += 1
        time.sleep(1.1)
        n2 = changed_count(s)
        if n2 > n0:
            placed = (dy, n2 - n0)
            break
    log("手机端触摸", "按住 0.7 秒只放一个方块（旧版放完又挖）", bool(placed) and placed[1] == 1,
        "改动 %d → %d%s" % (n0, n2, "" if placed else "（换了 5 个俯角都没放下）"))

    # 慢慢拖右半屏：只转视角，不该白扔一个方块
    n3 = changed_count(s)
    a = s.ev("return window.__T.sig();")
    s.ev("window.__T.tstart(%d, %d, %d);" % (tid, right[0], right[1])); time.sleep(0.12); tid += 1
    for k in range(1, 15):
        s.ev("window.__T.tmove(%d, %d, %d);" % (tid - 1, right[0] - 6 * k, right[1] - 4 * k))
        time.sleep(0.08)
    s.ev("window.__T.tend(%d, %d, %d);" % (tid - 1, right[0] - 84, right[1] - 56))
    time.sleep(1.0)
    n4 = changed_count(s)
    b = s.ev("return window.__T.sig();")
    log("手机端触摸", "慢速拖右半屏只转视角、不白掉方块", n4 == n3,
        "改动 %d → %d，视角变化 Δratio=%.4f" % (n3, n4, abs(b["ratio"] - a["ratio"]) if "err" not in a and "err" not in b else -1))

    # 双击 = 飞行（旧版说明里写着、代码里没做）
    t0 = time.time()
    tap(tid, right[0], right[1], gap=0.1, after=0.16); tid += 1
    tap(tid, right[0], right[1], gap=0.1, after=1.0); tid += 1
    mode = hud_field(s, 2)
    log("手机端触摸", "双击屏幕会开飞行（旧版说明里有、代码里没有）", "飞行" in mode,
        "模式=%s（两下间隔 %.2f 秒）" % (mode, time.time() - t0))
    tap(tid, right[0], right[1], gap=0.1, after=0.16); tid += 1
    tap(tid, right[0], right[1], gap=0.1, after=1.0); tid += 1
    log("手机端触摸", "再双击会关掉飞行", "飞行" not in hud_field(s, 2), "模式=%s" % hud_field(s, 2))

    # HUD 按钮桥：跳 / 换方块 / 飞（旧版手机上这三件事根本没有入口）
    p0 = s.ev("return window.__T.pos();")
    s.ev("document.getElementById('btn-jump').click();")
    time.sleep(0.6)
    p1 = s.ev("return window.__T.pos();")
    log("手机端触摸", "点 HUD「跳」按钮真的会跳（旧版手机没有跳的入口）", bool(p0 and p1 and p1[1] > p0[1] + 0.08),
        "y %.2f → %.2f" % (p0[1], p1[1]) if p0 and p1 else "读不到位姿")
    slot0 = hud_field(s, 0)
    s.ev("document.getElementById('btn-slot-next').click();")
    time.sleep(0.8)
    slot1 = hud_field(s, 0)
    log("手机端触摸", "点 HUD「下一个方块」会换方块（旧版手机换不了）", slot0 != slot1,
        "槽位 %s → %s，方块名=%s" % (slot0, slot1, hud_field(s, 1)))
    s.ev("document.getElementById('btn-fly').click();")
    time.sleep(0.8)
    log("手机端触摸", "点 HUD「飞」按钮会切飞行", "飞行" in hud_field(s, 2), "模式=%s" % hud_field(s, 2))

    errs = s.ev("return window.__T.errs();")
    log("手机端触摸", "整条触摸流程没有报错（老内核 TouchList 不可迭代会刷错）", not errs, "｜".join(errs[:3])[:200])
    warn = s.ev("return window.__T.warns();")
    log("手机端触摸", "手机端也没有截断警告", warn == 0, "%d 条" % warn)


def scene_soak(u, s=None):
    own = s is None
    if own:
        s = Sess(hook=HOOK)
        s.go(u + "/game/index.html?autoplay=1")
        s.ev(TOOL)
        wait_boot(s)
        wait_hud(s)
    fps = []
    t0 = time.time()
    while time.time() - t0 < 60:
        fps.append(hud_field(s, 5))
        time.sleep(2.0)
    nums = []
    for f in fps:
        m = re.search(r"(\d+)", f or "")
        if m:
            nums.append(int(m.group(1)))
    bad = [f for f in fps if not re.search(r"\d+", f or "")]
    log("长跑 60 秒", "帧率一直在动、没冻住", bool(nums) and not bad, "最后几个读数 %s%s" % (nums[-5:], "（有空读数）" if bad else ""))
    low = [n for n in nums[-10:] if n < 30]
    log("长跑 60 秒", "后段帧率不低于 30", not low, "低于 30 的读数 %s（全程 %d 个采样）" % (low, len(nums)))
    errs = s.ev("return window.__T.errs();")
    log("长跑 60 秒", "全程没有报错", not errs, "｜".join(errs[:3])[:160])
    warn = s.ev("return window.__T.warns();")
    log("长跑 60 秒", "全程没有截断警告", warn == 0, "%d 条" % warn)
    if own:
        s.close()


def scene_static(u):
    txt = {}
    for name in ("loader.js", "game.css", "index.html"):
        with urllib.request.urlopen(u + "/game/" + name, timeout=20) as r:
            txt[name] = r.read().decode("utf-8")
    L, C, I = txt["loader.js"], txt["game.css"], txt["index.html"]
    stripped = re.sub(r"/\*[\s\S]*?\*/|//[^\n]*", " ", L)
    stripped = re.sub(r"\'[^\']*\'|\"[^\"]*\"", "\"\"", stripped)
    es5 = sorted(set(re.findall(r"=>|`|\bconst \w|\blet \w|\?\?|\bclass \w", stripped)))
    ctx = ""
    for tok in es5[:3]:
        m = re.search(re.escape(tok), stripped)
        if m:
            ctx += "｜%s 出现在 %r" % (tok, stripped[max(0, m.start() - 40):m.start() + 24].strip())
    log("静态检查", "loader.js 守住 ES5 写法（老微信内核）", not es5, "去掉注释和字符串之后还剩 %s%s" % (es5 or "无", ctx))
    fimp = re.search(r"<input[^>]*file-import[^>]*>", I)
    # 凡是写了 clip-path 的规则，选择器必须落在 ::before/::after 上，否则画布四角点不到
    Cnc = re.sub(r"/\*[\s\S]*?\*/", " ", C)   # 注释里也会写 clip-path 这个词，别被它骗
    clip_bad = [sel.strip().splitlines()[-1] for sel, body in re.findall(r"([^{}]+)\{([^{}]*)\}", Cnc)
                if "clip-path" in body and re.search(r"\.g-frame\b|#glcanvas\b", sel) and "::" not in sel]
    checks = [
        ("IndexedDB 打开有超时", re.search(r"rejectFn[\s\S]{0,400}?setTimeout|setTimeout[\s\S]{0,300}?rejectFn", L) is not None),
        ("IndexedDB 连接复用（不是每 3 秒开一个新连接）", re.search(r"if\s*\(dbConn\)\s*return", L) is not None),
        ("清档/导入等写完才刷新", re.search(r"reloadAfterWrite\s*\(\s*(clearSave|putSave)", L) is not None),
        ("清档/导入期间挡住游戏自己再写档", "suppress" in L.lower()),
        ("重试会打散缓存（带 retry 参数）", "retry=" in L and "retry=" in I),
        ("导出用了 target=_blank（微信 iOS 才有 QuickLook/分享）", "_blank" in L),
        ("导出不再无条件宣称成功", "多半" in L or "如果没" in L),
        ("导入前会核存档格式（MCL1）", "MCL1" in L),
        ("跑之前先探 WebGL 并给中文原因", "webgl" in L.lower() and "WebGL" in L),
        ("触屏判据不止 ontouchstart（平板、带触屏的笔记本也算）", "any-pointer: coarse" in L),
        ("跳/飞/换方块那一排只在触屏判据成立时才显出来",
         re.search(r"if \(touchish\) \{[\s\S]{0,400}?hud-btns[\s\S]{0,200}?acts\.hidden = false", L) is not None),
        ("TouchList 不可迭代的老内核有补丁", "TouchList" in L and "Symbol.iterator" in L),
        ("wasm 起不来有看门狗：会截 console.error 并把原因写上门面", re.search(r"console\.error\s*=", L) is not None),
        ("HUD 只在值变了才写 DOM（旧版每 200ms 无脑重写）", re.search(r"textContent\s*!==|textContent\s*!=\s*[a-z]", L) is not None),
        ("HUD 映射了第 8 段（hud-note）", "hud-note" in I and "hud-note" in L),
        ("手机按钮的桥（跳/飞/换方块）三个导入都提供了", all(k in L for k in ("mcl_take_jump", "mcl_take_fly", "mcl_take_slot"))),
        ("取存档改成按偏移分块读（旧版 >256KB 会静默截断）", re.search(r"mcl_pull_save\s*=\s*function\s*\(\s*\w+\s*,\s*\w+\s*,\s*\w+", L) is not None and "mcl_save_len" in L),
        ("hidden 的 display 覆盖规则还在", "[hidden]" in C and "important" in C),
        ("门面改成顶部可达 + 能竖向滚", "flex-start" in C and "pan-y" in C),
        ("iPhone 安全区补上了", "env(safe-area-inset" in C),
        ("长按不会窜出选区/高亮", "-webkit-touch-callout" in C and "user-select" in C and "-webkit-tap-highlight-color" in C),
        ("画布用 fixed 钉住（不会被橡皮筋带着跳）", re.search(r"\.game-body[^}]*position:\s*fixed", C) is not None),
        ("页脚有最小高度（提示文案不会撑得画面重建）", re.search(r"\.g-foot[^}]*min-height", C) is not None),
        ("数字用等宽数字，HUD 不会抖行", "tabular-nums" in C),
        ("剪角只剪背景层，不再剪掉画布的点击区", not clip_bad and "clip-path" in C,
         "还剪在真元素上的选择器：%s" % (clip_bad or "无")), 
        ("viewport 禁了双击缩放", "maximum-scale=1" in I and "user-scalable=no" in I),
        ("文件选择框不再是 display:none（部分安卓 WebView 打不开）",
         bool(fimp) and "accept=" not in fimp.group(0) and "hidden" not in fimp.group(0)),
        ("blob 地址建不出来时有旁路（退回 manifest 给的原始 wasm 地址）", "objUrl || wasmUrl" in L),
        ("字体晚到/旋转后会补一次尺寸同步", "fonts.ready" in L or "fonts && document.fonts" in L),
    ]
    for item in checks:
        log("静态检查", item[0], item[1], item[2] if len(item) > 2 else "")
    return len([1 for it in checks if it[1] is True]), len(checks)


SCENES = {
    "base": scene_base,
    "nowebgl": scene_nowebgl,
    "watchdog": scene_watchdog,
    "gate": scene_gate,
    "touch": scene_touch,
    "static": scene_static,
}


def main():
    if not os.path.isdir(DIST):
        raise SystemExit("没有 dist/，先跑构建：cd blog && bash game/build-wasm.sh && python3 build.py")
    port = wdshot.free_port(8099)
    handler = functools.partial(SimpleHTTPRequestHandler, directory=DIST)
    httpd = ThreadingHTTPServer(("127.0.0.1", port), handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    wdshot.wait_http("http://127.0.0.1:%d/game/index.html" % port)
    u = "http://127.0.0.1:%d" % port
    print("服务 dist/ 于 %s" % u, flush=True)
    wdshot.ensure_driver(DRIVER)
    want = ONLY or list(SCENES) + ["soak"]
    try:
        for name in want:
            print("\n=== %s ===" % name, flush=True)
            for sess in SESSIONS:          # 每个场景都得有自己的会话，上一个必须先关掉
                sess.close()
            SESSIONS[:] = []
            try:
                if name == "soak":
                    scene_soak(u)
                else:
                    SCENES[name](u)
            except BaseException:
                import traceback
                traceback.print_exc()
                log(name, "这个场景自己崩了（不算产品的错，但这条得修）", False, "见上面的回溯")
    finally:
        for sess in SESSIONS:
            try:
                sess.close()
            except BaseException:
                pass
        httpd.shutdown()
    bad = [c for c in CHECKS if c[2] is False]
    skipped = [c for c in CHECKS if c[2] is None]
    print("\n" + "=" * 62)
    print("总计 %d 条，失败 %d 条，不作数 %d 条" % (len(CHECKS), len(bad), len(skipped)))
    for sc, name, ok, note in bad:
        print("  ✗ [%s] %s｜%s" % (sc, name, note))
    print("=" * 62)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
