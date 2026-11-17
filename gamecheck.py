#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""方块彩蛋自检：真的把 wasm 跑起来，看它到底起没起。

用法：
  python3 gamecheck.py                      # 截到 /home/suyzz/blogshot/game-check.png
  python3 gamecheck.py /tmp/别处.png
  GAME_URL=https://wkgn1.github.io/game/index.html?autoplay=1 python3 gamecheck.py

为什么要有这个脚本：这个彩蛋跑不跑只看页面是看不出来的——wasm 起不来时页面上
还是那块「点开玩」的牌子，长得跟没点一样。这里把 wasm 真跑起来，然后量四件事：
这台机器有没有 WebGL、__MCL_BOOTED 有没有置位、HUD 有没有在跳、门面收没收起。

依赖：geckodriver + snap 版 firefox（和 wdshot.py 同一套），只用标准库。
"""
import base64, os, pathlib, shutil, socket, subprocess, sys, time, urllib.request

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import wdshot                                                   # 复用 call / ensure_driver / wait_http

COMMON = pathlib.Path.home() / "snap/firefox/common"           # snap 版 Firefox 只写得了这里


def free_port():
    s = socket.socket(); s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]; s.close(); return p


def serve(dist):
    """就地起一个静态服务，测完关掉。"""
    port = free_port()
    proc = subprocess.Popen([sys.executable, "-m", "http.server", "--bind", "127.0.0.1",
                             str(port), "-d", str(dist)],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        wdshot.wait_http("http://127.0.0.1:%d/game/manifest.json" % port, secs=20)
    except SystemExit:
        proc.terminate(); raise SystemExit("静态服务起不来，或者 dist/game/ 是空的（先跑 build.py）")
    return proc, port


PROBE_HUD = "return String(window.__MCL_HUD || '');"

PROBE = """
var out = {booted: false, hud: '', gate: null, fallback: null, fps: '',
           webgl: '不知道', iw: window.innerWidth, ih: window.innerHeight};
try { out.booted = !!window.__MCL_BOOTED; } catch (e) {}
try { out.hud = String(window.__MCL_HUD || ''); } catch (e) {}
try { var g = document.getElementById('gate'); out.gate = g ? (!g.hidden) : null; } catch (e) {}
try { var f = document.getElementById('fallback'); out.fallback = f ? (!f.hidden && f.textContent.trim() !== '') : null; } catch (e) {}
try { var c = document.createElement('canvas');
      out.webgl = c.getContext('webgl2') ? 'webgl2' : (c.getContext('webgl') ? 'webgl1' : '没有');
} catch (e) { out.webgl = '探测报错 ' + e.name; }
return out;
"""


def main():
    out_png = sys.argv[1] if len(sys.argv) > 1 else "/home/suyzz/blogshot/game-check.png"
    dist = HERE / "dist"
    external = os.environ.get("GAME_URL")
    drv = os.environ.get("WDDRIVER", "http://127.0.0.1:4451")

    srv = None
    if external:
        url = external
    else:
        if not (dist / "game" / "index.html").is_file():
            raise SystemExit("dist/game/index.html 不在——先 python3 build.py")
        srv, port = serve(dist)
        url = "http://127.0.0.1:%d/game/index.html?autoplay=1" % port

    mine, dv = wdshot.ensure_driver(drv)
    prof = COMMON / ("ffprof-gamecheck-%d" % int(time.time()))
    prof.mkdir(parents=True, exist_ok=True)
    sid = ""
    try:
        caps = {"capabilities": {"alwaysMatch": {
            "browserName": "firefox", "pageLoadStrategy": "eager", "unhandledPromptBehavior": "ignore",
            "moz:firefoxOptions": {"args": ["-headless", "-no-remote", "-profile", str(prof),
                                            "-width", os.environ.get("GAME_W", "1100"), "-height", os.environ.get("GAME_H", "760")],
                                   "prefs": {"privacy.resistFingerprinting": False}}},
            "firstMatch": [{}]}}
        sid = wdshot.call(drv, "POST", "/session", caps)["value"]["sessionId"]

        def ev(script):
            return wdshot.call(drv, "POST", "/session/%s/execute/sync" % sid,
                               {"script": script, "args": []})["value"]

        wdshot.call(drv, "POST", "/session/%s/url" % sid, {"url": url}, timeout=120)
        t0 = time.time()
        got = {}
        while time.time() - t0 < 100:
            try:
                got = ev(PROBE)
            except SystemExit as exc:
                got = {"err": str(exc)[:200]}
            if got.get("booted"):
                break
            time.sleep(1.0)

        # 起来了再多跑几秒，让区块网格建出来、HUD 里的帧率动起来
        if got.get("booted"):
            for _ in range(6):
                time.sleep(1.0); got = ev(PROBE)
        # —— 真挖一下：HUD 的「改动」不动就说明输入没接到 wasm ——
        def changed():
            h = str(ev(PROBE_HUD))
            parts = h.split("|")
            try:
                return int(parts[4])
            except Exception:
                return -1

        dig = {"试法": "", "之前": 0, "之后": 0, "坐标": (0, 0)}
        if got.get("booted"):
            cx, cy = ev("var r=document.getElementById('glcanvas').getBoundingClientRect();"
                        "return [Math.round(r.left+r.width/2), Math.round(r.top+r.height/2)];")
            dig["坐标"] = (cx, cy)
            dig["之前"] = changed()
            click = ("var x=%d,y=%d,c=document.getElementById('glcanvas');"
                     "function mm(t,b,d){c.dispatchEvent(new MouseEvent(t,{clientX:x,clientY:y,button:b,buttons:d?1:0,bubbles:true,cancelable:true,view:window}));}"
                     "function pp(t,b,d){try{c.dispatchEvent(new PointerEvent(t,{clientX:x,clientY:y,button:b,buttons:d?1:0,pointerId:1,pointerType:'mouse',isPrimary:true,bubbles:true,cancelable:true,view:window}));}catch(e){mm(t,b,d);}}"
                     "pp('pointermove',0,false);mm('mousemove',0,false);"
                     "pp('pointerdown',0,true);mm('mousedown',0,true);"
                     "setTimeout(function(){pp('pointerup',0,false);mm('mouseup',0,false);},120);return 1;") % (cx, cy)
            ev(click)
            time.sleep(2.0)
            dig["之后"] = changed()
            dig["试法"] = "JS 合成点击"
            if dig["之后"] <= dig["之前"]:
                # 合成事件不认就走真的指针动作：先把视线往下压一点再挖（射线可能打空）
                wdshot.call(drv, "POST", "/session/%s/actions" % sid, {"actions": [
                    {"type": "pointer", "id": "m", "parameters": {"pointerType": "mouse"}, "actions": [
                        {"type": "pointerMove", "duration": 60, "x": cx, "y": cy},
                        {"type": "pointerDown", "button": 0},
                        {"type": "pointerMove", "duration": 220, "x": cx, "y": max(cy - 70, 5)},
                        {"type": "pointerUp", "button": 0}]}]}, timeout=40)
                time.sleep(0.6)
                wdshot.call(drv, "POST", "/session/%s/actions" % sid, {"actions": [
                    {"type": "pointer", "id": "m", "parameters": {"pointerType": "mouse"}, "actions": [
                        {"type": "pointerMove", "duration": 60, "x": cx, "y": cy},
                        {"type": "pointerDown", "button": 0},
                        {"type": "pause", "duration": 120},
                        {"type": "pointerUp", "button": 0}]}]}, timeout=40)
                time.sleep(2.0)
                dig["之后"] = changed()
                dig["试法"] = "W3C 指针动作"

        blob = base64.b64decode(wdshot.call(drv, "GET", "/session/%s/screenshot" % sid, timeout=60)["value"])
        pathlib.Path(out_png).write_bytes(blob)

        hud = got.get("hud", "")
        parts = hud.split("|")
        fps = parts[5] if len(parts) > 5 else ""
        changed = parts[4] if len(parts) > 4 else ""
        print("URL            :", url)
        print("WebGL          :", got.get("webgl"))
        print("已就绪 BOOTED  :", got.get("booted"), "（等了 %.1f 秒）" % (time.time() - t0))
        print("门面还盖着     :", got.get("gate"), "｜兜底文案露出 :", got.get("fallback"))
        print("HUD            :", hud or "(空的)")
        print("HUD 帧率/改动  :", fps, "/", changed)
        print("窗口           :", got.get("iw"), "x", got.get("ih"))
        print("截图           :", out_png, wdshot.png_size(blob))
        dug = dig.get("之后", 0) > dig.get("之前", 0)
        print("挖方块测试     : %s｜改动 %s → %s｜用的办法 %s｜点位 %s"
              % ("动了" if dug else "没动", dig.get("之前"), dig.get("之后"), dig.get("试法") or "没试", dig.get("坐标")))
        ok = bool(got.get("booted")) and got.get("gate") is False and hud and got.get("fallback") is not True and dug
        print("结论           :", "PASS 彩蛋能跑能挖" if ok else "FAIL 彩蛋没正常跑起来")
        if not ok:
            sys.exit(2)
    finally:
        try:
            if sid:
                wdshot.call(drv, "DELETE", "/session/%s" % sid, timeout=15)
        except BaseException:
            pass
        shutil.rmtree(prof, ignore_errors=True)
        if srv:
            srv.terminate()
        if mine:
            dv.terminate()


if __name__ == "__main__":
    main()
