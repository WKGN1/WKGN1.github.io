#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""JS 语法门禁（只用标准库 + npx 里的 acorn）。

为什么要有这个文件：微信安卓端内置浏览器用的是腾讯 X5 内核，很多机器上还停在
Chromium 77；iOS 微信是系统 Safari，老设备上低于 13.1。这两种内核都不认识
可选链 `?.` / 空值合并 `??` / 无参数 `catch {}`，遇到就在**解析阶段**整份报错，
于是 fx.js 里的 wireTheme() 根本没跑，页面上「切换深浅」的按钮就成了摆设——
而 CSS、variant.js、?theme=dark 全都照常，看着就像"只有按钮坏了"。

规则（和 base.html 的兜底脚本一起构成防线）：
  assets/fx.js            ≤ ES2017（模块）
  assets/variant.js       ≤ ES2015（普通脚本）
  templates/*.html 内联   ≤ ES5（type="module" 的那块 ≤ ES2015 模块）

用法：
  python3 check-es.py            # 全过打 PASS，有不过打明细并返回码 1
"""
import pathlib
import re
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent
FILES = [("assets/fx.js", 2017, True), ("assets/variant.js", 2015, False)]
INLINE_LIMIT = 5      # 页面内联脚本按 ES5 卡
MODULE_INLINE_LIMIT = 2015
SCRIPT_RE = re.compile(r"<script([^>]*)>(.*?)</script>", re.S)
TAG_OK = ("templates",)


def acorn(text, ecma, module, where):
    """把一段 JS 交给 acorn 按指定版本解析；成功 True，失败返回错误串。"""
    with tempfile.NamedTemporaryFile("w", suffix=".js", delete=False, encoding="utf-8") as fh:
        fh.write(text)
        path = fh.name
    cmd = ["npx", "--yes", "acorn@8", f"--ecma{ecma}", "--silent"]
    if module:
        cmd.append("--module")
    cmd += ["--", path]
    try:
        done = subprocess.run(cmd, capture_output=True, text=True, timeout=180)
    except FileNotFoundError:
        return "找不到 npx（离线或没装 node）"
    except subprocess.TimeoutExpired:
        return "acorn 超时（网络不通？npx 拉包要联网）"
    finally:
        try:
            pathlib.Path(path).unlink()
        except OSError:
            pass
    if done.returncode == 0:
        return True
    msg = (done.stderr or done.stdout or "").strip().splitlines()
    head = msg[0] if msg else "acorn 返回码 %d" % done.returncode
    return "%s（ES%d%s）%s" % (where, ecma, "/module" if module else "", head)


def inline_scripts(html_path):
    """产出 (标识, 代码, 是否模块, 限制版本)。src 引用的块跳过。"""
    text = html_path.read_text(encoding="utf-8")
    for n, (attrs, body) in enumerate(SCRIPT_RE.findall(text), 1):
        if "src=" in attrs:
            continue
        if not body.strip():
            continue
        is_mod = 'type="module"' in attrs or "type='module'" in attrs
        yield ("%s#%d" % (html_path.name, n), body, is_mod,
               MODULE_INLINE_LIMIT if is_mod else INLINE_LIMIT)


def main():
    problems = []
    checked = 0

    for rel, limit, is_mod in FILES:
        p = ROOT / rel
        if not p.exists():
            problems.append("%s 不存在" % rel)
            continue
        res = acorn(p.read_text(encoding="utf-8"), limit, is_mod, rel)
        checked += 1
        if res is not True:
            problems.append(res)

    for p in sorted((ROOT / "templates").glob("*.html")):
        for where, body, is_mod, limit in inline_scripts(p):
            checked += 1
            res = acorn(body, limit, is_mod, where)
            if res is not True:
                problems.append(res)

    for line in problems:
        print("FAIL  " + line)
    if problems:
        print("门禁没过：%d/%d 处超了语法上限" % (len(problems), checked))
        print("改法：把 ?. / ?? / catch {} 换成普通 if + try…catch(e)，别升语法。")
        return 1
    print("PASS  已检查 %d 处 JS，全部在语法上限内（fx.js≤2017，variant.js≤2015，内联≤ES5）" % checked)
    return 0


if __name__ == "__main__":
    sys.exit(main())
