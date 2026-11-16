#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把生成好的页面打成"单文件预览版"：CSS、字体、脚本、背景线稿全部塞进一个 .html。
用途：在没有网络、或者打不开外网链接的手机上，直接双击/用浏览器打开就能看效果。
  python3 bundle.py        → dist/预览-首页.html 和 dist/预览-文章.html
站内的跳转链接会被改成 #（因为别的页面不在这个文件里），其余和正式版一模一样。
"""
import base64, pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent
DIST = ROOT / "dist"
A = DIST / "assets"

def datauri(path, mime):
    return "data:%s;base64,%s" % (mime, base64.b64encode(path.read_bytes()).decode())

def inline_css(css_text):
    css_text = css_text.replace("url('Saira.ttf')", "url('%s')" % datauri(A / "Saira.ttf", "font/ttf"))
    for name in ("contour-a", "contour-b"):
        p = A / "contours" / (name + ".svg")
        if p.exists():
            css_text = css_text.replace("url('./contours/%s.svg')" % name, "url('%s')" % datauri(p, "image/svg+xml"))
    return css_text

def inline_js(js_text):
    # 注释里出现过 </script> 这种字样，直接内联会被 HTML 提前闭合，所以把 </ 转义成 <\/
    return re.sub(r"^export ", "", js_text, flags=re.M).replace("</", "<\\/")

PAGES = [("index.html", "预览-首页.html"),
         ("posts.html", "预览-文章列表.html"),
         ("kb.html", "预览-知识库.html"),
         ("about.html", "预览-关于.html")]

def pick_post():
    posts = sorted(DIST.glob("p-*.html"))
    return posts[0].name if posts else None

def bundle(src, dst, note):
    html = (DIST / src).read_text(encoding="utf-8")
    css = inline_css((A / "endfield.css").read_text(encoding="utf-8") + "\n" + (A / "site.css").read_text(encoding="utf-8"))
    html = re.sub(r'\s*<link rel="stylesheet" href="assets/endfield.css" />', "", html)
    html = re.sub(r'\s*<link rel="stylesheet" href="assets/site.css" />', "", html)
    html = re.sub(r'\s*<link rel="preload" as="font"[^>]*/>', "", html)
    html = html.replace("</head>", "    <style>\n%s\n    </style>\n  </head>" % css, 1)
    html = re.sub(r'\s*<script src="assets/variant.js"></script>', "", html)
    html = html.replace("</head>", "    <script>\n%s\n    </script>\n  </head>" % inline_js((A / "variant.js").read_text(encoding="utf-8")), 1)
    html = re.sub(r"<script type=\"module\">.*?</script>",
                  lambda m: "<script>\n%s\nwireReadouts();wireTheme();wireParallax(document.querySelector('[data-hero]'));\n</script>" % inline_js((A / "fx.js").read_text(encoding="utf-8")),
                  html, count=1, flags=re.S)
    # 站内跳转改成 #，只保留页面内锚点、外部链接和 mailto
    html = re.sub(r'href="((?:index|posts|kb|about|p-|n-)[^"]*\.html)"', 'href="#"', html)
    banner = ('    <div class="ef-field" style="position:sticky;top:0;z-index:9;background:var(--ef-signal);'
              'color:var(--ef-on-signal);padding:0.45rem var(--ef-pad);text-align:center">%s</div>\n' % note)
    html = re.sub(r"(<body[^>]*>\n)", r"\1" + banner, html, count=1)
    (DIST / dst).write_text(html, encoding="utf-8")
    return (DIST / dst).stat().st_size

if __name__ == "__main__":
    if not DIST.exists():
        sys.exit("先跑 python3 build.py")
    note = "单文件预览版｜不能点跳转｜正式版等 GitHub 地址"
    made = []
    for src, dst in PAGES[:1] + ([("posts.html", "预览-文章列表.html")]):
        made.append((dst, bundle(src, dst, note)))
    post = pick_post()
    if post:
        made.append(("预览-一篇文章.html", bundle(post, "预览-一篇文章.html", note)))
    for dst, n in made:
        print("%s  %.1f KB" % (dst, n / 1024.0))
