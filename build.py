#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
零依赖静态站点生成器：只用 python3 标准库。
  改内容：往 content/posts 或 content/notes 里放 .md 文件
  改样式/名字/强调色：改 site.json 和 assets/site.css
  生成：python3 build.py   → dist/
"""
import json, re, html, pathlib, datetime, collections, email.utils, urllib.parse

ROOT = pathlib.Path(__file__).resolve().parent
TPL = ROOT / "templates"
TPL_CACHE = {}

def tpl(name):
    if name not in TPL_CACHE:
        TPL_CACHE[name] = (TPL / name).read_text(encoding="utf-8")
    return TPL_CACHE[name]

def render(name, **kw):
    out = tpl(name)
    for k, v in kw.items():
        out = out.replace("{{%s}}" % k, "" if v is None else str(v))
    return out

# ---------------------------------------------------------------- markdown
def esc(s):
    # quote=True：这些转义后的值既进正文也进属性（href/content="…"），
    # 标题或 URL 里出现引号时不会把标签提前闭合。
    return html.escape(s)

def xesc(s):
    """XML（RSS）用的转义：& < > 和引号都转，中文保持原样（文件本身是 UTF-8）。"""
    return html.escape(str(s))

def site_base(conf):
    """绝对地址的前缀：有自定义域名就用它，否则用 site.json 里的 base。"""
    dom = (conf.get("domain") or "").strip()
    if not dom:
        return (conf.get("base") or "").rstrip("/")
    if dom.startswith("http://") or dom.startswith("https://"):
        return dom.rstrip("/")
    return "https://" + dom.lstrip("/").rstrip("/")

def inline(s):
    s = esc(s)
    s = re.sub(r"`([^`]+)`", r"<code>\1</code>", s)
    s = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", s)
    s = re.sub(r"(?<!\*)\*([^*]+)\*(?!\*)", r"<em>\1</em>", s)
    s = re.sub(r"!\[([^\]]*)\]\(([^)\s]+)[^)]*\)", r'<img src="\2" alt="\1">', s)
    s = re.sub(r"\[([^\]]+)\]\(([^)\s]+)[^)]*\)", r'<a href="\2">\1</a>', s)
    return s

def slugify(s):
    s = re.sub(r"[^\w\u4e00-\u9fff-]+", "-", s.strip().lower()).strip("-")
    return s or "doc"

BLOCK_START = re.compile(r"^(#{1,4}\s|```|>|\||\s*(?:[-*+]|\d+\.)\s)")
HR = re.compile(r"^\s*([-*_])\s*\1\s*\1\s*$")

def md2html(md):
    """支持：标题 / 段落 / 列表 / 引用 / 代码块 / 表格 / 分隔线。返回 (html, [(level,text,id)])"""
    md = md.replace("\r\n", "\n")
    blocks, heads = [], []
    i, lines = 0, md.split("\n")

    def anchor(text):
        sid = slugify(re.sub(r"`", "", text))[:60]
        n = sum(1 for h in heads if h[2].startswith(sid))
        sid = sid + ("-%d" % n if n else "")
        return sid

    while i < len(lines):
        ln = lines[i]

        if ln.startswith("```"):
            lang = ln[3:].strip()
            i += 1; buf = []
            while i < len(lines) and not lines[i].startswith("```"):
                buf.append(lines[i]); i += 1
            i += 1
            blocks.append('<pre><code class="lang-%s">%s</code></pre>' % (esc(lang or "text"), esc("\n".join(buf))))
            continue

        m = re.match(r"^(#{1,4})\s+(.*)$", ln)
        if m:
            lvl = len(m.group(1)); text = m.group(2).strip()
            sid = anchor(text)
            heads.append((lvl, text, sid))
            tag = "h2" if lvl == 2 else ("h3" if lvl == 3 else "h4")
            blocks.append('<%s id="%s">%s</%s>' % (tag, sid, inline(text), tag))
            i += 1; continue

        if re.match(r"^\s*([-*_])\s*\1\s*\1\s*$", ln):
            blocks.append("<hr>"); i += 1; continue

        if ln.startswith(">"):
            buf = []
            while i < len(lines) and lines[i].startswith(">"):
                buf.append(lines[i][1:].lstrip()); i += 1
            blocks.append("<blockquote><p>%s</p></blockquote>" % inline(" ".join(buf)))
            continue

        if ln.startswith("|") and i + 1 < len(lines) and re.match(r"^\|[\s:|-]+\|$", lines[i + 1]):
            rows = []
            while i < len(lines) and lines[i].startswith("|"):
                rows.append(lines[i]); i += 1
            cells = lambda r: [c.strip() for c in r.strip().strip("|").split("|")]
            head = cells(rows[0]); body = [cells(r) for r in rows[2:]]
            blocks.append("<div class='ef-table-wrap'><table class='ef-table'><thead><tr>%s</tr></thead><tbody>%s</tbody></table></div>" % (
                "".join("<th>%s</th>" % inline(c) for c in head),
                "".join("<tr>%s</tr>" % "".join("<td>%s</td>" % inline(c) for c in r) for r in body)))
            continue

        m = re.match(r"^\s*([-*+]|\d+\.)\s+", ln)
        if m:
            ordered = m.group(1)[0].isdigit()
            tag = "ol" if ordered else "ul"
            items = []
            while i < len(lines) and re.match(r"^\s*([-*+]|\d+\.)\s+", lines[i]):
                item = [re.sub(r"^\s*([-*+]|\d+\.)\s+", "", lines[i])]
                i += 1
                while i < len(lines) and (lines[i].startswith("  ") or lines[i].startswith("\t")) and lines[i].strip():
                    item.append(lines[i].strip()); i += 1
                items.append("<li>%s</li>" % inline(" ".join(item)))
            blocks.append("<%s>%s</%s>" % (tag, "".join(items), tag))
            continue

        if not ln.strip():
            i += 1; continue

        buf = []
        while i < len(lines) and lines[i].strip() and not BLOCK_START.match(lines[i]) and not HR.match(lines[i]):
            buf.append(lines[i].strip()); i += 1
        blocks.append("<p>%s</p>" % inline(" ".join(buf)))

    return "\n".join(blocks), heads

FRONT = re.compile(r"^---\n(.*?\n)---\n", re.S)

def read_doc(path, kind):
    raw = path.read_text(encoding="utf-8")
    meta = {}
    m = FRONT.match(raw)
    if m:
        for line in m.group(1).strip().split("\n"):
            if ":" in line:
                k, v = line.split(":", 1)
                meta[k.strip()] = v.strip()
        raw = raw[m.end():]
    body_md = raw
    body_html, heads = md2html(body_md)
    plain = re.sub(r"`|```|\*\*|^#+\s*|\[[^\]]*\]\([^)]*\)|!\[[^\]]*\]\([^)]*\)", lambda x: "", body_md)
    words = wordcount(plain)
    date = meta.get("date") or ""
    slug = path.stem
    return {
        "kind": kind, "slug": slug, "file": path.name,
        "title": meta.get("title") or path.stem,
        "date": date, "updated": meta.get("updated") or "",
        "tags": [t.strip() for t in meta.get("tags", "").split(",") if t.strip()],
        "category": meta.get("category") or ("文章" if kind == "post" else "笔记"),
        "summary": meta.get("summary") or "",
        "body": body_html, "heads": heads, "words": words,
        "readmin": max(1, round(words / 350)),
        "draft": meta.get("draft", "").lower() in ("1", "true", "yes"),
    }

def wordcount(t):
    cjk = len(re.findall(r"[\u3040-\u30ff\u4e00-\u9fff]", t))
    latin = len(re.findall(r"[A-Za-z0-9][A-Za-z0-9'\-\.\+/]*", t))
    return cjk + latin

# ---------------------------------------------------------------- helpers
def num(n):
    return format(n, ",d")

def fmt_date(d):
    return d.replace("-", ".") if d else ""

def load(conf):
    docs = []
    for kind, sub in (("post", "posts"), ("note", "notes")):
        d = ROOT / "content" / sub
        if d.is_dir():
            for p in sorted(d.glob("*.md")):
                if p.stem.startswith("_"):
                    continue
                docs.append(read_doc(p, kind))
    docs.sort(key=lambda x: (x["date"], x["slug"]), reverse=True)
    for n, doc in enumerate(docs, 1):
        doc["no"] = "%03d" % (len(docs) - n + 1)
    posts = [d for d in docs if d["kind"] == "post" and not d["draft"]]
    notes = [d for d in docs if d["kind"] == "note" and not d["draft"]]
    total_words = sum(d["words"] for d in docs)
    days = 0
    if conf.get("since"):
        try:
            days = (datetime.date.today() - datetime.date(*map(int, conf["since"].split("-")))).days
        except Exception:
            days = 0
    cats = collections.OrderedDict()
    for d in docs:
        cats.setdefault(d["category"], []).append(d)
    tags = collections.Counter(t for d in docs for t in d["tags"])
    stats = {"docs": len(docs), "posts": len(posts), "notes": len(notes), "words": total_words,
             "cats": len(cats), "days": days, "cats_map": cats, "tags": tags,
             "updated": max([d["date"] for d in docs if d["date"]] or [""])}
    return docs, posts, notes, stats

def rail(conf, active, root_rel=False):
    items = [("index.html", "首页"), ("posts.html", "文章"), ("kb.html", "知识库"), ("about.html", "关于")]
    pre = "/" if root_rel else ""      # 404 可能在深层路径下被打开，链接要从站点根走
    links = []
    for href, name in items:
        cur = ' aria-current="page"' if href == active else ""
        links.append('<a href="%s%s"%s>%s</a>' % (pre, href, cur, esc(name)))
    return render("rail.html", BRAND=conf["brand"], BRAND_LINK=pre + "index.html", LINKS="".join(links))

def records(docs, conf, show_cat=False):
    out = []
    for i, d in enumerate(docs):
        tail = d["words"] and "%s 字" % num(d["words"])
        extra = esc(d["category"]) if show_cat else ""
        out.append('<li><a class="ef-record ef-in-come" style="--i:%d" href="%s">'
                   '<span class="ef-tag">No.%s</span><time class="ef-field">%s</time>'
                   '<span class="name ef-read">%s</span><span class="ef-field">%s%s</span></a></li>'
                   % (i, link(d), d["no"], fmt_date(d["date"]), esc(d["title"]), extra, tail))
    return "".join(out)

def link(d):
    # 页面全部平铺在 dist/ 里，所以站内链接都是同级，改结构时不用管相对路径
    return ("p-" if d["kind"] == "post" else "n-") + d["slug"] + ".html"

def abs_url(conf, fname):
    """站内某个文件对应的绝对地址。percent-encode 过：文章名可能是中文，
       直接写进 sitemap / canonical / og:url 里，严格的解析器会翻脸。
       首页的规范地址就是站点根，别再挂一个 index.html（同一页两个地址会被当成重复内容）。"""
    if str(fname) == "index.html":
        return site_base(conf) + "/"
    return site_base(conf) + "/" + urllib.parse.quote(str(fname), safe="/")

def page(name, body, conf, active, title=None, meta_desc="", fname="index.html",
         og_type="website", og_article="", root_rel=False):
    """公共头尾。fname 决定 canonical / og:url 指向哪个地址；
       root_rel=True 给 404 这种"可能在任意深层路径下被打开"的页面用，
       那时相对路径会算错，资源和链接一律从站点根走。"""
    pre = "/assets/" if root_rel else "assets/"          # MODULE 必须是 './assets/' 或 '/assets/'
    img = abs_url(conf, (conf.get("share_image") or "assets/share-card.png").lstrip("/"))
    return render("base.html", ASSETS=pre, MODULE=("./" + pre) if not root_rel else pre,
                  FEED="/feed.xml" if root_rel else "feed.xml",
                  CANON=esc(abs_url(conf, fname)), OG_TYPE=og_type, OG_IMAGE=esc(img),
                  OG_ARTICLE=og_article,
                  VARIANT=conf.get("variant", "site"), BRAND=esc(conf["brand"]),
                  TITLE=esc(title or conf["brand"]), DESC=esc(meta_desc or conf["tagline"]),
                  HEAD_EXTRA=conf.get("head_extra", ""), BODY=body, ACTIVE=active)

# ---------------------------------------------------------------- pages
def home(conf, docs, posts, notes, stats, built):
    latest = (posts or notes or docs)[:1]
    hero_docs = docs[:8]
    ticker = "".join('<li><a href="%s"><b>No.%s</b>%s</a></li>' % (link(d), d["no"], esc(d["title"])) for d in hero_docs)
    ticker2 = ticker.replace("<li>", "<li>", 1)

    skills = "".join(
        '<li><a href="about.html">%s</a><span class="ef-field">%s</span></li>' % (esc(s["name"]), esc(s["level"]))
        for s in conf.get("skills", []))

    idx_rows = []
    for cat, ds in stats["cats_map"].items():
        idx_rows.append('<h2 class="ef-group-head ef-read"><span>%s</span><span class="ef-field">%d 篇 · %s 字</span></h2>'
                        % (esc(cat), len(ds), num(sum(d["words"] for d in ds))))
        idx_rows.append('<ol class="ef-records">%s</ol>' % records(ds[:8], conf))
        if len(ds) > 8:
            idx_rows.append('<p class="ef-actions"><a class="ef-btn small" href="%s">全部 %d 篇</a></p>'
                            % ("kb.html" if ds[0]["kind"] == "note" else "posts.html", len(ds)))

    body = render("home.html",
                  TODAY=fmt_date(datetime.date.today().isoformat()),
                  DAYS=num(stats["days"]), DOCS=num(stats["docs"]), WORDS=num(stats["words"]),
                  NOTES=num(stats["notes"]), CATS=num(stats["cats"]),
                  SOLID=esc(conf["wordmark_solid"]), HOLLOW=esc(conf["wordmark_hollow"]),
                  CLAIM=esc(conf["claim"]), BIO=esc(conf["bio"]), BRAND=esc(conf["brand"]),
                  LATEST_NO=(latest[0]["no"] if latest else "000"),
                  FRAME=render("frame.html", PLATE=esc(conf.get("motto", ""))),
                  TICKER=ticker, TICKER2=ticker2,
                  L_NO=(latest[0]["no"] if latest else "000"),
                  L_DATE=fmt_date(latest[0]["date"]) if latest else "",
                  L_WORDS=("%s 字" % num(latest[0]["words"])) if latest else "",
                  L_READ=("约 %d 分钟" % latest[0]["readmin"]) if latest else "",
                  L_TITLE=esc(latest[0]["title"]) if latest else "还没有内容",
                  L_LINK=link(latest[0]) if latest else "posts.html",
                  L_SUM=esc(latest[0]["summary"] or re.sub("<[^>]+>", "", latest[0]["body"])[:80]) if latest else "往 content/posts 里放一个 .md 文件就有了",
                  SKILLS=skills, SKILL_N=str(len(conf.get("skills", []))),
                  INDEX="".join(idx_rows), BUILT=fmt_date(built))
    return page("index", body, conf, "index.html", title=conf["brand"], meta_desc=conf["tagline"])

def doc_page(conf, d, docs, stats, active=None):
    same = [x for x in docs if x is not d and (set(x["tags"]) & set(d["tags"]) or (x["kind"] == d["kind"] and x["category"] == d["category"]))][:3]
    i = docs.index(d)
    prevn, nextp = (docs[i + 1] if i + 1 < len(docs) else None), (docs[i - 1] if i else None)
    toc_items = "".join('<li><a href="#%s">%s</a></li>' % (sid, esc(t)) for lv, t, sid in d["heads"] if lv <= 3)
    toc = ('<h2 class="margin-h ef-field">目录</h2><ol>%s</ol>' % toc_items) if toc_items else ""
    # 分享平台认这几条：发布日期 + 标签（og:type=article 时在 head 里露出来）
    art = []
    if d["date"]:
        art.append('<meta property="article:published_time" content="%s" />' % esc(d["date"]))
    for t in d["tags"][:6]:
        art.append('<meta property="article:tag" content="%s" />' % esc(t))
    cats = "".join('<li><a href="kb.html">%s</a><span class="ef-field">%d 篇</span></li>'
                   % (esc(c), len(v)) for c, v in stats["cats_map"].items())
    tags = "".join('<li><span>%s</span><span class="ef-field">%d</span></li>' % (esc(t), n)
                   for t, n in list(stats["tags"].most_common(8)))
    body = render("doc.html",
                  EYEBROW=esc(("Posts / Records" if d["kind"] == "post" else "Knowledge / Notes")),
                  CRUMB_TEXT=esc("全部文章" if d["kind"] == "post" else "知识库"),
                  CRUMB_LINK="posts.html" if d["kind"] == "post" else "kb.html",
                  NO=d["no"], CAT=esc(d["category"]),
                  DATE=fmt_date(d["date"]), WORDS=num(d["words"]), READ="约 %d 分钟" % d["readmin"],
                  UPD=('<li>更新 %s</li>' % fmt_date(d["updated"])) if d["updated"] else "",
                  TITLE=esc(d["title"]),
                  LEDE=('<p class="ef-lede ef-read">%s</p>' % esc(d["summary"])) if d["summary"] else "",
                  BODY=d["body"], TOC=toc, CATS=cats, TAGS=tags,
                  TOTALWORDS="%s 字" % num(stats["words"]),
                  RELATED=('<h2 class="ef-group-head ef-read"><span>相关</span><span class="ef-field">%d 篇</span></h2>'
                           '<ol class="ef-records">%s</ol>' % (len(same), records(same, conf, True))) if same else
                          '<p class="ef-empty">还没有同分类或同标签的内容，写完就会出现在这里。</p>',
                  PREV=('<a class="ef-btn small" href="%s">上一篇 No.%s</a>' % (link(prevn), prevn["no"])) if prevn else "",
                  NEXT=('<a class="ef-btn small" href="%s">下一篇 No.%s</a>' % (link(nextp), nextp["no"])) if nextp else "",
                  RAIL=rail(conf, active or ("posts.html" if d["kind"] == "post" else "kb.html")))
    return page(d["slug"], body, conf, active, title=d["title"] + " · " + conf["brand"],
                meta_desc=d["summary"] or d["title"], fname=link(d), og_type="article",
                og_article="\n    ".join(art))

def list_page(conf, docs_subset, stats, kind, title, sub):
    groups = collections.OrderedDict()
    for d in docs_subset:
        groups.setdefault(d["category"], []).append(d)
    blocks = []
    for cat, ds in groups.items():
        blocks.append('<h2 class="ef-group-head ef-read"><span>%s</span><span class="ef-field">%d 篇 · %s 字</span></h2>'
                      % (esc(cat), len(ds), num(sum(d["words"] for d in ds))))
        blocks.append('<ol class="ef-records">%s</ol>' % records(ds, conf))
    if not blocks:
        blocks.append('<p class="ef-empty">这里还是空的。往 content/%s 放一个 .md 文件，再跑一次 build.py 就有了。</p>' % kind)
    months = collections.Counter(d["date"][:7] for d in docs_subset if d["date"])
    bym = "".join('<li><a href="#">%s</a><span class="ef-field">%d 篇</span></li>' % (m.replace("-", "."), n)
                  for m, n in sorted(months.items(), reverse=True))
    body = render("list.html", EYEBROW=esc(sub), TITLE=esc(title), N="%d 篇" % len(docs_subset),
                  BLOCKS="".join(blocks), MONTHS=bym or "<li><span>暂无</span></li>",
                  TOTALS="".join('<li><span>%s</span><span class="ef-field">%s 字</span></li>' % (esc(c), num(sum(d["words"] for d in v)))
                                 for c, v in stats["cats_map"].items()),
                  RAIL=rail(conf, "posts.html" if kind == "posts" else "kb.html"))
    return page(kind + "_index", body, conf, "posts.html" if kind == "posts" else "kb.html",
                title=title + " · " + conf["brand"], fname="posts.html" if kind == "posts" else "kb.html")

def about_page(conf, docs, stats):
    d = read_doc(ROOT / "content" / "about.md", "post") if (ROOT / "content" / "about.md").exists() else None
    if not d:
        d = {"slug": "about", "no": "---", "date": "", "updated": "", "category": "关于", "tags": [],
             "summary": conf["tagline"], "body": "", "heads": [], "words": 0, "readmin": 1}
    links = "".join('<li><a href="%s">%s</a><span class="ef-field">%s</span></li>' % (esc(l.get("url", "#")), esc(l.get("name", "")), esc(l.get("key", "")))
                    for l in conf.get("links", []))
    skills = "".join('<li><a href="#">%s</a><span class="ef-field">%s</span></li>' % (esc(s["name"]), esc(s["level"]))
                     for s in conf.get("skills", []))
    toc_items = "".join('<li><a href="#%s">%s</a></li>' % (sid, esc(t)) for lv, t, sid in d["heads"] if lv <= 3)
    body = render("about.html", NO="%03d" % (stats["docs"] + 1), BODY=d["body"],
                  TITLE=esc(d["title"]),
                  LEDE=('<p class="ef-lede ef-read">%s</p>' % esc(d["summary"])) if d["summary"] else "",
                  TOC=('<h2 class="margin-h ef-field">目录</h2><ol>%s</ol>' % toc_items) if toc_items else "",
                  LINKS=links, SKILLS=skills,
                  STATS=('<li><span>文章</span><span class="ef-field">%d 篇</span></li>'
                         '<li><span>知识库</span><span class="ef-field">%d 条</span></li>'
                         '<li><span>总字数</span><span class="ef-field">%s 字</span></li>'
                         '<li><span>最近更新</span><span class="ef-field">%s</span></li>'
                         % (stats["posts"], stats["notes"], num(stats["words"]), fmt_date(stats["updated"]))))
    return page("about", body, conf, "about.html", title="关于 · " + conf["brand"], fname="about.html")

# -------------------------------------------------- 404 页 / 机器人 / RSS 订阅
CST = datetime.timezone(datetime.timedelta(hours=8))   # 作者在这头，RSS 里的时间按这个算

def not_found(conf, docs, stats):
    """GitHub Pages 找不到页面时会端出 dist/404.html（用户站生效）。
       这一页可能在任意深层地址下被打开，所以资源和链接一律从站点根走（root_rel）。
       右边那几个数字是真数字，别写死。"""
    live = [d for d in docs if not d["draft"]]
    ledger = ('<li><span>文章</span><span class="ef-field">%d 篇</span></li>\n'
              '              <li><span>知识库</span><span class="ef-field">%d 条</span></li>\n'
              '              <li><span>总字数</span><span class="ef-field">%s 字</span></li>'
              % (stats["posts"], stats["notes"], num(stats["words"])))
    body = render("404.html", RAIL=rail(conf, "", root_rel=True), BRAND=conf["brand"],
                  CLAIM=esc(conf.get("claim", "")), LEDGER=ledger, N=num(len(live)),
                  LEDE="页面可能改过名，或者链接少抄了一段——已经发布的文章不会换地址，所以这里多半是链接本身写错了。",
                  WHY="东西都在文章列表和知识库里，找不到就回首页。这一页也是 build.py 生成的：要改文案改 not_found()，要改排布改 templates/404.html。")
    return page("404", body, conf, "", title="页面没找到 · " + conf["brand"],
                meta_desc="这个地址下没有东西：去文章列表、知识库或首页看看",
                fname="404.html", root_rel=True)

def robots_txt(conf):
    return ("# 这个站由 build.py 生成，这里也是。允许所有爬虫，sitemap 在最后一行。\n"
            "User-agent: *\nAllow: /\n\nSitemap: %s\n" % abs_url(conf, "sitemap.xml"))

def plain_text(d, limit=140):
    """RSS 的 description 要纯文本：正文是 HTML，先把标签去掉、再把实体还原回 & 和 <。"""
    t = html.unescape(re.sub(r"<[^>]+>", " ", d["body"]))
    t = re.sub(r"\s+", " ", t).strip()
    if not t:
        return ""
    return (t[:limit] + "…") if len(t) > limit else t

def rss_feed(conf, docs, stats):
    base = site_base(conf)
    live = [d for d in docs if not d["draft"]]          # 草稿不进订阅
    items = []
    for d in live:
        url = abs_url(conf, link(d))
        pub = ""
        try:
            dt = datetime.datetime.strptime(d["date"], "%Y-%m-%d").replace(tzinfo=CST)
            pub = "  <pubDate>%s</pubDate>\n" % email.utils.format_datetime(dt)
        except ValueError:
            pass                                          # 没写日期就不发 pubDate，别编一个
        seen, cats = set(), []
        for c in [d["category"]] + d["tags"]:                # 分类和标签常重名，去过重再发
            if c and c not in seen:
                seen.add(c); cats.append(c)
        cat_xml = "".join("  <category>%s</category>\n" % xesc(c) for c in cats[:6])
        items.append("<item>\n  <title>%s</title>\n  <link>%s</link>\n"
                     "  <guid isPermaLink=\"true\">%s</guid>\n%s%s  <description>%s</description>\n</item>\n"
                     % (xesc(d["title"]), xesc(url), xesc(url), pub, cat_xml,
                        xesc(d["summary"] or plain_text(d))))
    card = abs_url(conf, (conf.get("share_image") or "assets/share-card.png").lstrip("/"))
    now = datetime.datetime.now(CST)
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">\n<channel>\n'
        "  <title>%s</title>\n  <link>%s</link>\n  <description>%s</description>\n"
        "  <language>zh-CN</language>\n  <lastBuildDate>%s</lastBuildDate>\n"
        '  <atom:link href="%s" rel="self" type="application/rss+xml" />\n'
        "  <image>\n    <url>%s</url>\n    <title>%s</title>\n    <link>%s</link>\n"
        "    <width>1200</width>\n    <height>630</height>\n  </image>\n%s</channel>\n</rss>\n"
        % (xesc(conf.get("tagline") or conf["brand"]), xesc(abs_url(conf, "index.html")),
           xesc(conf.get("bio") or conf.get("tagline", "")),
           email.utils.format_datetime(now), xesc(abs_url(conf, "feed.xml")),
           xesc(card), xesc(conf["brand"]), xesc(abs_url(conf, "index.html")), "".join(items)))

# ---------------------------------------------------------------- main
def main():
    conf = json.load(open(ROOT / "site.json", encoding="utf-8"))
    built = datetime.date.today().isoformat()
    docs, posts, notes, stats = load(conf)

    out = ROOT / "dist"
    for sub in ("assets/contours", "posts", "notes"):
        (out / sub).mkdir(parents=True, exist_ok=True)
    import shutil
    # 运行时用不到的东西别发布：tokens.json 只是给改样式的人看的色值表（12KB），
    # variant.js 里只在注释里提到它。要留就写进 NO_PUBLISH 之外。
    NO_PUBLISH = {"tokens.json"}
    for p in (ROOT / "assets").iterdir():
        if p.is_file() and p.name not in NO_PUBLISH:
            shutil.copy2(p, out / "assets" / p.name)
    for p in (ROOT / "assets" / "contours").glob("*"):
        shutil.copy2(p, out / "assets" / "contours" / p.name)
    # static/ 里的东西原样进 dist 根（favicon.ico 这类"客户端直接去根上取"的文件放这里）
    static = ROOT / "static"
    if static.is_dir():
        for p in static.rglob("*"):
            if p.is_file():
                dst = out / p.relative_to(static)
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(p, dst)

    # 图片放 content/img/，会自动拷到 dist/img/，文章里写 ![说明](img/xxx.png) 就能显示
    imgs = ROOT / "content" / "img"
    if imgs.is_dir():
        (out / "img").mkdir(parents=True, exist_ok=True)
        for p in imgs.iterdir():
            if p.is_file():
                shutil.copy2(p, out / "img" / p.name)

    write(out / "index.html", home(conf, docs, posts, notes, stats, built))
    write(out / "posts.html", list_page(conf, posts, stats, "posts", "全部文章", "Posts / Records"))
    write(out / "kb.html", list_page(conf, notes, stats, "notes", "知识库", "Knowledge / Notes"))
    write(out / "about.html", about_page(conf, docs, stats))
    for d in docs:
        write(out / link(d), doc_page(conf, d, docs, stats))
    write(out / "404.html", not_found(conf, docs, stats))
    write(out / "robots.txt", robots_txt(conf))
    write(out / "feed.xml", rss_feed(conf, docs, stats))
    dom = (conf.get("domain") or "").strip()
    if dom:
        (out / "CNAME").write_text(dom + "\n", encoding="utf-8")
    # 站点地图里别出现草稿；地址一律 percent-encode（文章名可能是中文）
    urls = ["index.html", "posts.html", "kb.html", "about.html"] + [link(d) for d in docs if not d["draft"]]
    (out / "sitemap.xml").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
        + "".join('<url><loc>%s</loc></url>\n' % xesc(abs_url(conf, p)) for p in urls) + "</urlset>\n",
        encoding="utf-8")

    print("生成完成：%d 个页面｜文章 %d｜知识库 %d｜字数 %s｜附 404 页 / feed.xml / robots.txt｜dist 在 %s"
          % (4 + len(docs), len(posts), len(notes), num(stats["words"]), out))

def write(path, text):
    path.write_text(text, encoding="utf-8")

if __name__ == "__main__":
    main()
