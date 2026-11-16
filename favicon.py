#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""站点图标生成器：一份几何，同时出 SVG、各尺寸 PNG 和 favicon.ico。

  python3 favicon.py
    → assets/favicon.svg            浏览器标签用的矢量图
    → assets/favicon-32.png         老内核（只认位图的那些）用的
    → assets/apple-touch-icon.png   手机加到桌面用的
    → static/favicon.ico            爬虫和不读 <link> 的老客户端直接取 /favicon.ico

图标只在**改形状时**才需要跑这个脚本，build.py 不需要 Pillow（图标生成好就提交进仓库）。
想换样子：只改下面 GEOM 里的坐标和两个颜色，重跑本脚本，别手改生成的文件。

形状本身是站内风格的压缩版：一个右上角切角的方块（切角只在可点击的东西上出现，
图标算一个入口），底色用深色暖纸（浅色标签栏和深色标签栏都看得见），
里面三条琥珀色横杠 + 一个小方块，就是站内"记录清单"那几行的缩略。
"""
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent
V = 512                    # 设计稿边长（viewBox 就是这个数）
BG = "#1b1a17"             # 深色暖纸：站点 paper=warm 的深色档
FG = "#ffb31f"             # 琥珀：站点 accent=amber 的强调色

# 一份几何，两边（SVG 和 PNG）都用它，不会走偏。
BADGE = [(40, 40), (376, 40), (472, 96), (472, 472), (40, 472)]   # 右上角切 96
BARS = [(96, 168, 288, 40), (96, 248, 200, 40), (96, 328, 112, 40)]   # x, y, w, h
TAG = (96, 404, 40, 40)                                            # 记录行左边那个小方块
FIELD = (152, 414, 148, 20)                                        # 小方块后面那根细的

SS = 12                    # 位图按 12 倍超采样画，再缩下来，边缘才干净


def svg_text():
    pts = " ".join("%d,%d" % p for p in BADGE)
    bars = "".join('<rect x="%d" y="%d" width="%d" height="%d"/>' % b for b in BARS)
    tag = '<rect x="%d" y="%d" width="%d" height="%d"/>' % TAG
    field = '<rect x="%d" y="%d" width="%d" height="%d" opacity=".55"/>' % FIELD
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d" width="%d" height="%d">\n'
        '  <path d="%s Z" fill="%s"/>\n  <g fill="%s">%s%s%s</g>\n</svg>\n'
        % (V, V, V, V, "M" + " L".join("%d %d" % p for p in BADGE), BG, FG, bars, tag, field)
    )


def raster(size, ss=SS):
    """画一个 size x size 的位图（先放大 ss 倍再缩，抗锯齿靠这个）。"""
    from PIL import Image, ImageDraw

    k = size * ss
    s = lambda v: int(round(v * k / float(V)))
    img = Image.new("RGBA", (k, k), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    rgb = lambda hexs: tuple(int(hexs[i:i + 2], 16) for i in (1, 3, 5))
    bg, fg = rgb(BG), rgb(FG)
    # SVG 里那根细的用 opacity=.55，位图这边就直接把底色和琥珀混成同一个颜色
    faint = tuple(int(round(b + (f - b) * 0.55)) for (b, f) in zip(bg, fg))
    d.polygon([(s(x), s(y)) for (x, y) in BADGE], fill=bg)
    for (x, y, w, h) in list(BARS) + [TAG]:
        d.rectangle([s(x), s(y), s(x + w), s(y + h)], fill=fg)
    x, y, w, h = FIELD
    d.rectangle([s(x), s(y), s(x + w), s(y + h)], fill=faint)
    return img.resize((size, size), Image.LANCZOS)


def main():
    out = ROOT / "assets"
    static = ROOT / "static"
    static.mkdir(exist_ok=True)
    (out / "favicon.svg").write_text(svg_text(), encoding="utf-8")
    raster(32).save(out / "favicon-32.png")
    raster(180).save(out / "apple-touch-icon.png")
    base = raster(256)
    base.save(static / "favicon.ico", format="ICO", sizes=[(16, 16), (32, 32), (48, 48), (256, 256)])
    for p in (out / "favicon.svg", out / "favicon-32.png", out / "apple-touch-icon.png", static / "favicon.ico"):
        print("%-28s %6.1f KB" % (p.relative_to(ROOT), p.stat().st_size / 1024.0))


if __name__ == "__main__":
    import sys
    try:
        main()
    except ImportError:
        sys.exit("这个脚本只是画图用的工具，需要 Pillow：python3 -m pip install pillow（build.py 不需要）")
