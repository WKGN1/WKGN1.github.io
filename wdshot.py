#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""无头截图（实测用）：geckodriver + 每次全新 profile + 等字体到位 + 把窗口差值补回来。

用法：
  python3 wdshot.py http://127.0.0.1:8093/404.html 404.png            # 默认 1280x1000
  python3 wdshot.py https://wkgn1.github.io/ 首页.png 1280 900
  python3 sharecard.py                                                # 卡片也走这一套

为什么不用 `firefox --screenshot`：那条 CLI 只等 load 事件，字体可能还在路上，
量出来的宽度/排版是系统字体顶出来的假值（上一轮就被这个骗过）。这里等
document.fonts.status=loaded 再拍，并且每回换一个新 profile，免得 assets/*.css|js 被缓存住、
改动看着像没生效。

为什么窗口尺寸要"补差值再重开"：这台机器上的 headless Firefox 改不动内容尺寸
（POST /session/../window 拿真的窗口 handle 返回成功，innerWidth/innerHeight 一动不动；
handle 用 root / parent / window 都是 404 no such window）。所以先按目标尺寸开窗口，
量出差值，把差值加到 -width/-height 上重开一个会话，最多三次。

前提：本机有 geckodriver（snap 装在 /snap/bin/geckodriver）。没有就 WDDRIVER 指一个现成的。
只用 Python 标准库。
"""
import base64, json, os, shutil, struct, subprocess, sys, time, urllib.error, urllib.request

PROFILE_ROOT = os.path.expanduser("~/snap/firefox/common")     # snap 版 Firefox 只写得了这里


def free_port(prefer):
    """要一个能用的端口：先试点名的，被占就去问内核要一个。"""
    import socket
    if prefer:
        with socket.socket() as s:
            try:
                s.bind(("127.0.0.1", prefer))
                return prefer
            except OSError:
                pass
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def call(driver, method, path, payload=None, timeout=90):
    """给 geckodriver 发一条 WebDriver 命令；HTTP 错误直接 SystemExit 并带上返回体。"""
    data = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request(driver + path, data=data, method=method,
                                 headers={"Content-Type": "application/json", "Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            raw = r.read().decode()
            return json.loads(raw) if raw else {}
    except urllib.error.HTTPError as exc:
        raise SystemExit("%s %s -> HTTP %d: %s" % (method, path, exc.code, exc.read().decode()[:400]))


def wait_http(url, secs=25):
    deadline = time.time() + secs
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=2) as r:
                if r.status < 500:
                    return True
        except Exception:
            time.sleep(0.3)
    raise SystemExit("等不到它起来：%s" % url)


def ensure_driver(driver):
    """没在跑就起一个。返回 (要不要由我们关, 进程或 None)。"""
    try:
        call(driver, "GET", "/status", timeout=3)
        return False, None
    except Exception:
        pass
    exe = os.environ.get("GECKODRIVER") or "geckodriver"
    port = driver.rsplit(":", 1)[-1]
    try:
        dv = subprocess.Popen([exe, "--port", port], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except OSError:
        raise SystemExit("起不来 geckodriver（%s）。装一个，或用 WDDRIVER 指一个现成的。" % exe)
    try:
        wait_http(driver + "/status")
    except SystemExit:
        dv.terminate()
        raise SystemExit("geckodriver 起来了但没应答（%s:%s）" % (driver, port))
    return True, dv


PROBE = ("var out={fonts:'?',iw:0,ih:0};try{out.fonts=document.fonts?document.fonts.status:'没有 document.fonts';}"
         "catch(e){out.fonts='报错:'+e.name;} out.iw=window.innerWidth;out.ih=window.innerHeight;return out;")


def capture(driver, url, out, width=1280, height=1000, verbose=True, attempts=3):
    """把 url 截成 width x height 的 PNG 写到 out。截不到位就返回 ok=False（别拿这张当结论）。"""
    out = str(out)
    profs, sid, got, dw, dh, ok = [], "", {}, 0, 0, False

    def ev(script, s):
        return call(driver, "POST", "/session/%s/execute/sync" % s, {"script": script, "args": []})["value"]

    try:
        for n in range(1, attempts + 1):
            prof = os.path.join(PROFILE_ROOT, "ffprof-wdshot-%d-%d" % (int(time.time()), n))
            os.makedirs(prof, exist_ok=True)
            profs.append(prof)
            caps = {"capabilities": {"alwaysMatch": {
                "browserName": "firefox", "pageLoadStrategy": "none", "unhandledPromptBehavior": "ignore",
                "moz:firefoxOptions": {"args": ["-headless", "-no-remote", "-profile", prof,
                                                "-width", str(width + dw), "-height", str(height + dh)],
                                       "prefs": {"privacy.resistFingerprinting": False}}},
                "firstMatch": [{}]}}
            sid = call(driver, "POST", "/session", caps)["value"]["sessionId"]
            call(driver, "POST", "/session/%s/url" % sid, {"url": url})
            for _ in range(40):                                   # 先把页面停下来
                if ev("return document.readyState;", sid) in ("interactive", "complete"):
                    break
                time.sleep(0.25)
            for _ in range(40):                                   # 字体没到位就别拍：宽度会是假的
                got = ev(PROBE, sid)
                if got["fonts"] == "loaded":
                    break
                time.sleep(0.25)
            time.sleep(0.35)                                      # 字体刚到位，再等一下让它真的画上
            got = ev(PROBE, sid)
            if verbose:
                print("第 %d 次：窗口开 %dx%d → 内容 %dx%d｜字体 %s"
                      % (n, width + dw, height + dh, got["iw"], got["ih"], got["fonts"]), flush=True)
            if (got["iw"], got["ih"]) == (width, height):
                blob = base64.b64decode(call(driver, "GET", "/session/%s/screenshot" % sid)["value"])
                with open(out, "wb") as fh:
                    fh.write(blob)
                ok = got["fonts"] == "loaded" and png_size(blob) == (width, height)
                break
            dw, dh = width - got["iw"], height - got["ih"]
            try:
                call(driver, "DELETE", "/session/%s" % sid, timeout=15)
            except BaseException:
                pass
            sid = ""
            time.sleep(0.6)                                       # 上一个 Firefox 退出要一点点时间
        return {"ok": ok, "out": out, "inner": [got.get("iw", 0), got.get("ih", 0)], "fonts": got.get("fonts", "?")}
    finally:
        # SystemExit 不是 Exception 的派生类：收尾这里要连它一起吞，
        # 否则收尾的报错会把正文真正的失败原因顶掉（上一轮只看得见一句 405，查了半天）。
        try:
            if sid:
                call(driver, "DELETE", "/session/%s" % sid, timeout=15)
        except BaseException:
            pass
        for p in profs:                                           # 临时 profile 别堆在 ~/snap 里
            try:
                shutil.rmtree(p)
            except OSError:
                pass


def png_size(blob):
    """从 PNG 头里读宽高，确认截出来的就是要点的那个尺寸，别信工具说的话。"""
    import struct
    if blob[:8] != b"\x89PNG\r\n\x1a\n":
        return (0, 0)
    return struct.unpack(">II", blob[16:24])


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    driver = os.environ.get("WDDRIVER", "http://127.0.0.1:4445")
    url, out = sys.argv[1], sys.argv[2]
    w = int(sys.argv[3]) if len(sys.argv) > 3 else 1280
    h = int(sys.argv[4]) if len(sys.argv) > 4 else 1000
    mine, dv = ensure_driver(driver)
    try:
        r = capture(driver, url, out, w, h)
        print("%s %s｜%dx%d｜字体 %s" % ("OK" if r["ok"] else "没到位", r["out"], r["inner"][0], r["inner"][1], r["fonts"]))
        if not r["ok"]:
            sys.exit(2)
    finally:
        if mine:
            dv.terminate()


if __name__ == "__main__":
    main()
