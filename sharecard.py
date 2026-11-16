#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""分享卡片生成器：把 templates/sharecard.html 真渲染、真跑字体，截成 1200×630 的图。

  python3 sharecard.py                    → assets/share-card.png（1200×630）
  python3 sharecard.py --theme light      → 浅色版（默认 dark：琥珀在深底上更跳）
  python3 sharecard.py --out /tmp/x.png   → 试效果用，不覆盖仓库里那张

截图这套（等字体、每次新 profile、补窗口差值）在 wdshot.py 里，那条路踩过的坑都写在那儿。
为什么要真浏览器截图、不用 PIL 画：卡片要用站里同一套 CSS（切角、细线、琥珀、线稿），
手画会走偏；真渲染出来的就是线上那个样子。

要先生成站点：python3 build.py（卡片读 dist/assets 里的样式和字体）。
卡片页本身不发布：dist/share-card.html 只在本地，仓库里只有生成的那张 PNG。
分享平台的尺寸要求（1200×630）同时写在 base.html 的 og:image:width/height 里，改要一起改。
"""
import argparse, datetime, json, os, pathlib, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))
import build, wdshot

W, H = 1200, 630


def main():
    ap = argparse.ArgumentParser(description="生成分享卡片 assets/share-card.png")
    ap.add_argument("--out", default=str(ROOT / "assets" / "share-card.png"))
    ap.add_argument("--theme", default=None, choices=("dark", "light"),
                    help="深浅（默认 dark；也可在 site.json 里写 card_theme）")
    ap.add_argument("--driver", default=os.environ.get("WDDRIVER", "http://127.0.0.1:4445"))
    ap.add_argument("--port", type=int, default=8092, help="本地临时服务的端口（被占就自动换一个）")
    args = ap.parse_args()

    conf = json.load(open(ROOT / "site.json", encoding="utf-8"))
    docs, posts, notes, stats = build.load(conf)
    dist = ROOT / "dist"
    if not (dist / "assets" / "Saira.ttf").exists():
        raise SystemExit("先跑 python3 build.py（卡片要读 dist/assets 里的样式和字体）")

    theme = args.theme or conf.get("card_theme") or "dark"
    host = build.site_base(conf).split("://", 1)[-1]
    facts = ("<li>文章 <b>%s</b> 篇</li><li>知识库 <b>%s</b> 条</li>"
             "<li>共 <b>%s</b> 字</li><li>建站第 <b>%s</b> 天</li>"
             % (build.num(stats["posts"]), build.num(stats["notes"]),
                build.num(stats["words"]), build.num(stats["days"])))
    page = build.render("sharecard.html", ASSETS="assets/", VARIANT=conf.get("variant", "site"), THEME=theme,
                        EYEBROW=build.esc(conf.get("card_eyebrow") or "RECORDS · 本地大模型 · LINUX · 界面"),
                        SOLID=build.esc(conf["wordmark_solid"]), HOLLOW=build.esc(conf["wordmark_hollow"]),
                        CLAIM=build.esc(conf["claim"]), BIO=build.esc(conf["bio"]), FACTS=facts,
                        URL=build.esc(host),
                        UPDATED=build.fmt_date(stats["updated"] or datetime.date.today().isoformat()),
                        FRAME=build.render("frame.html", PLATE=build.esc(conf.get("motto", ""))))
    (dist / "share-card.html").write_text(page, encoding="utf-8")

    port = wdshot.free_port(args.port)
    srv = subprocess.Popen([sys.executable, "-m", "http.server", "-d", str(dist), str(port)],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    mine, dv = False, None
    try:
        wdshot.wait_http("http://127.0.0.1:%d/share-card.html" % port)
        mine, dv = wdshot.ensure_driver(args.driver)
        r = wdshot.capture(args.driver, "http://127.0.0.1:%d/share-card.html" % port, args.out, W, H)
        size = pathlib.Path(r["out"]).stat().st_size / 1024.0 if r["ok"] else 0
        print("%s：%s｜%.1f KB" % ("卡片" if r["ok"] else "卡片没截到位", r["out"], size))
        if not r["ok"]:
            sys.exit(2)
    finally:
        if mine and dv:
            dv.terminate()
        srv.terminate()


if __name__ == "__main__":
    main()
