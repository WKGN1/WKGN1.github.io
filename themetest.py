#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""主题切换自检：往 dist/ 里塞测试页，用 firefox --screenshot 直接看结论（绿=可用，红=坏了）。

firefox --screenshot 不支持延时，截图时机卡在 load 那一刻，所以测试脚本一律**同步**跑完：
fx.js 在有 View Transitions 的浏览器里会把 flip 推迟到过渡回调里执行，同步读不到结果，
于是各测试页先把它换成同步执行的形式（老内核本来就没有这个 API，走的就是同一条路）。

生成的页面：
  dist/t-nofx.html   抽掉 fx.js 模块脚本 → 只剩 base.html 里的 ES5 兜底。
                     模拟 fx.js 完全没跑成（老内核解析失败 / 模块报错 / 文件 404）。
  dist/t-novt.html   fx.js 在，但 document.startViewTransition 置为不存在
                     → 走 fx.js 自己「没有 VT 就直接 flip」那条路（老浏览器的真实路径）。
  dist/t-vt.html     fx.js 在，startViewTransition 换成「立刻执行回调」的假货
                     → 顺带把圆形展开那条分支的代码也跑一遍。

每页先清掉 localStorage 的 theme、去掉 data-theme，回到"第一次来"的状态再点两下，看
data-theme、token 取值（--ef-paper / --ef-ink，这两个不受颜色过渡影响，读数最准）、
body 背景（页面有 0.28s 颜色过渡，这一列可能还停在旧值，只作参考）、aria-pressed、有没有记住选择。
结论横幅先摆出来再填内容，自检脚本自己崩了也会留字。

用法：python3 themetest.py <dist 目录>
      firefox --no-remote --profile ~/snap/firefox/common/ffprof \
              --screenshot=<输出.png> --window-size=1280,760 "http://127.0.0.1:8088/t-nofx.html?noanim"
"""
import pathlib
import re
import sys

SETUPS = {
    "t-nofx": ("<!-- 这里本来该有 fx.js 的模块脚本，自检页故意抽掉 -->",
               "抽掉 fx.js：只剩 ES5 兜底"),
    "t-novt": ("    <script>try { document.startViewTransition = undefined; } catch (e) {}</script>\n",
               "fx.js 在，无 View Transitions（老浏览器路径）"),
    "t-vt": ("""    <script>
      document.startViewTransition = function (cb) {
        try { cb(); } catch (e) { window.__vtErr = String(e); }
        return { ready: Promise.resolve(), finished: Promise.resolve(), skipTransition: function () {} };
      };
    </script>
""", "fx.js 在，走圆形展开分支（VT 假货，回调同步执行）"),
}

TEST = """
    <script>
      (function () {
        var errs = [];
        window.addEventListener('error', function (e) {
          errs.push('JS 报错: ' + e.message + ' @ ' + (e.filename || '') + ':' + (e.lineno || ''));
        }, true);
        function box() {
          var el = document.querySelector('[data-verdict]');
          if (!el) {
            el = document.createElement('div');
            el.setAttribute('data-verdict', 'RUNNING');
            el.style.cssText = 'position:fixed;left:0;top:0;right:0;z-index:99999;background:#ffd23f;' +
              'color:#08090a;font:700 20px/1.45 system-ui,sans-serif;padding:12px 16px;white-space:pre-wrap';
            document.body.appendChild(el);
          }
          return el;
        }
        function show(verdict, lines) {
          var el = box();
          el.setAttribute('data-verdict', verdict);
          el.style.background = verdict === 'PASS' ? '#00ffa2' : (verdict === 'RUNNING' ? '#ffd23f' : '#ff2f4f');
          el.textContent = (verdict === 'PASS' ? '✅ 切换深浅可用｜' :
                            verdict === 'RUNNING' ? '⏳ 自检进行中｜' : '❌ 切换深浅坏了｜') +
            __MODE__ + '\\n' + lines.join('\\n');
          document.title = verdict + ' 主题切换自检';
        }
        function darkRules() {
          var n = 0, ss = document.styleSheets, i, j, rules, sel;
          for (i = 0; i < ss.length; i++) {
            try { rules = ss[i].cssRules || []; } catch (e) { rules = []; }
            for (j = 0; j < rules.length; j++) {
              sel = rules[j].selectorText || '';
              if (sel.indexOf("data-theme='dark'") >= 0 || sel.indexOf('data-theme="dark"') >= 0) n++;
            }
          }
          return n;
        }
        function tok(name) {
          return getComputedStyle(document.documentElement).getPropertyValue(name).replace(/^\\s+|\\s+$/g, '');
        }
        function sig() {
          void document.body.offsetHeight;              // 逼一次样式重算
          return '纸面(--ef-paper)=' + tok('--ef-paper') + ' 墨色(--ef-ink)=' + tok('--ef-ink') +
                 '  body背景=' + getComputedStyle(document.body).backgroundColor;
        }
        function theme() { return document.documentElement.getAttribute('data-theme') || '(无=跟随系统)'; }

        function run() {
          box();                                        // 先把横幅摆出来，崩了也看得见
          try { localStorage.removeItem('theme'); } catch (e) {}
          document.documentElement.removeAttribute('data-theme');   // 回到"第一次来"

          var b = document.querySelector('[data-theme-toggle]');
          var out = [], verdict = 'PASS';
          out.push('深色 CSS 规则条数（要 >0）: ' + darkRules());
          if (!b) {
            out.push('FAIL：页面上找不到 [data-theme-toggle]'); verdict = 'FAIL';
          } else {
            var aria0 = b.getAttribute('aria-pressed');
            var fb0 = b.getAttribute('data-theme-fallback');
            out.push('fx.js 接手（aria-pressed 有值）: ' + (aria0 !== null ? '是 → ' + aria0 : '否'));
            out.push('ES5 兜底接手（data-theme-fallback=1）: ' + (fb0 === '1' ? '是' : '否'));
            if (aria0 === null && fb0 !== '1') { out.push('FAIL：fx.js 和兜底都没绑上点击'); verdict = 'FAIL'; }

            var t0 = theme(), s0 = sig();
            b.click();
            var t1 = theme(), s1 = sig();
            var store = '(读不到)';
            try { store = String(localStorage.getItem('theme')); } catch (e) { store = 'localStorage 不可用'; }
            var aria1 = b.getAttribute('aria-pressed');
            b.click();
            var t2 = theme(), s2 = sig();

            out.push('点之前  data-theme=' + t0 + '   ' + s0);
            out.push('第 1 下  data-theme=' + t1 + '   ' + s1 + '   存下=' + store);
            out.push('第 2 下  data-theme=' + t2 + '   ' + s2);
            out.push('点后 aria-pressed=' + aria1);

            if (t1 === t0) { out.push('FAIL：第 1 下没改 data-theme'); verdict = 'FAIL'; }
            if (tok('--ef-paper') === s0.split('  ')[0] && t1 !== t0) {
              out.push('注意：token 又转回原值了'); verdict = 'FAIL';
            }
            if (s1 === s0) { out.push('FAIL：第 1 下样式没跟着走（token 没变）'); verdict = 'FAIL'; }
            if (t2 === t1) { out.push('FAIL：第 2 下没改回来'); verdict = 'FAIL'; }
            if (s2 === s1) { out.push('FAIL：第 2 下样式没跟着走'); verdict = 'FAIL'; }
            if (aria1 !== 'true' && aria1 !== 'false') { out.push('FAIL：aria-pressed 没设置（' + aria1 + '）'); verdict = 'FAIL'; }
            if (store !== 'light' && store !== 'dark') { out.push('FAIL：选择没记住（' + store + '）'); verdict = 'FAIL'; }
            if (window.__vtErr) { out.push('FAIL：VT 回调里报错 ' + window.__vtErr); verdict = 'FAIL'; }
          }
          if (errs.length) {
            out.push('有 JS 报错，多半就是这么坏的：');
            out = out.concat(errs);
            verdict = 'FAIL';
          }
          show(verdict, out);
        }
        try {
          if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', run);
          else run();
        } catch (e) {
          show('FAIL', ['自检脚本自己崩了: ' + e]);
        }
      })();
    </script>
  </body>
"""

MODULE_RE = re.compile(r"\s*<script type=\"module\">.*?</script>", re.S)

# 面包屑：先看"截图那一刻到底跑到哪一步"，免得把浏览器截图时机当成站点的毛病
BREADCRUMB = """    <script>
      (function () {
        var b = document.querySelector('[data-theme-toggle]');
        var el = document.createElement('div');
        el.style.cssText = 'position:fixed;left:0;bottom:0;right:0;z-index:99998;background:#8ecbff;' +
          'color:#08090a;font:700 17px/1.4 system-ui,sans-serif;padding:7px 14px';
        el.textContent = '面包屑@解析时 readyState=' + document.readyState + ' 按钮=' + (b ? '在' : '无') +
          ' aria-pressed=' + (b ? String(b.getAttribute('aria-pressed')) : '-') +
          ' 兜底=' + (b ? String(b.getAttribute('data-theme-fallback')) : '-');
        document.body.appendChild(el);
        document.addEventListener('DOMContentLoaded', function () {
          el.textContent += '   |   DCL时 aria-pressed=' + (b ? String(b.getAttribute('aria-pressed')) : '-') +
            ' 兜底=' + (b ? String(b.getAttribute('data-theme-fallback')) : '-');
        });
      })();
    </script>
"""


def build(html, key, drop_module):
    setup, mode = SETUPS[key]
    page = MODULE_RE.sub(setup, html, count=1) if drop_module else html
    head = "" if drop_module else setup          # VT 那段要在 fx.js 执行前先就位（普通脚本比模块先跑）
    return page.replace("</body>", head + BREADCRUMB + TEST.replace("__MODE__", "'%s'" % mode), 1)


def main():
    dist = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "dist").resolve()
    src = dist / "index.html"
    if not src.exists():
        sys.exit("先跑 python3 build.py（找不到 %s）" % src)
    html = src.read_text(encoding="utf-8")
    for key, drop in [("t-nofx", True), ("t-novt", False), ("t-vt", False)]:
        page = build(html, key, drop)
        p = dist / (key + ".html")
        p.write_text(page, encoding="utf-8")
        print("%-10s %4d KB  fx.js模块脚本=%s  ES5兜底=%s" % (
            key, len(page.encode()) // 1024,
            "在" if 'type="module"' in page else "已抽掉",
            "在" if "data-theme-fallback" in page else "缺"))


if __name__ == "__main__":
    main()
