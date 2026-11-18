# 我的站（终末地风味）

内容只有 Markdown，样式只有一套 CSS，生成只靠一个 Python 脚本。没有数据库、没有后台、没有依赖包。

## 三条命令

```bash
python3 build.py                                  # 生成 dist/
python3 -m http.server -d dist 8080                # 本地预览 http://127.0.0.1:8080
git add -A && git commit -m "写一篇" && git push    # 推到 GitHub 自动上线
```

## 每天怎么用

- **写文章**：复制 `content/_模板.md`，改名放进 `content/posts/`，头部填 `title`、`date`、`tags`、`summary`、`category`，跑一次 `python3 build.py`。
- **写知识库**：同上，放进 `content/notes/`。这一类会自动进「知识库」页。
- **不想发布**：头部加一行 `draft: true`，页面里不会出现，但字数照算。
- **文件名以下划线开头的**（比如 `_模板.md`）脚本会跳过，可以当模板存着。

## 想改什么，改哪里

| 想改 | 改哪 |
| --- | --- |
| 站名、简介、座右铭、导航、联系方式、技能清单 | `site.json`（只有一个文件） |
| 强调色、纸面冷暖、切角大小、密不密、动不动 | `site.json` 里 `"variant"`，可选 `site` / `standard` / `coldstore` / `nightshift` |
| 临时试效果 | 地址后缀 `?variant=coldstore`、`?theme=dark`、`?noanim` |
| 页面排布、字号、间距、响应式 | `assets/site.css` |
| 整套外观规则（颜色、切角、动效曲线） | `assets/endfield.css` 和 `assets/tokens.json` |
| 某一种页面的骨架 | `templates/home.html`（首页）、`templates/doc.html`（文章）、`templates/list.html`（列表）、`templates/about.html`（关于）、`templates/base.html`（公共头尾） |
| 统计代码、favicon、分享图 | `site.json` 里 `"head_extra"`，写一段 HTML |
| 文章里插图 | 图片放 `content/img/`，文章里写 `![说明](img/文件名.png)`，构建时自动拷过去 |

## 页面上那些数字

篇数、字数、阅读分钟、第几天、分类计数，都是脚本现算的：`第 N 天` 用 `site.json` 里的 `since` 算。这套风格要求**数字必须是真的**，所以别在页面里手写统计。

## 部署

**方案一：GitHub Pages + Actions（推荐，免费，改完 push 就上线）**
1. 在 GitHub 建一个公开仓库，把这些文件推上去（`main` 分支）。
2. 仓库 Settings → Pages → Source 选 **GitHub Actions**。
3. 以后只要 push，`dist/` 自动重建自动发布。
4. 有自己域名：把域名写进 `site.json` 的 `"domain"`，CNAME 会自动生成，再去域名解析里加一条 CNAME。

**方案二：直接把 `dist/` 扔上服务器**
`dist/` 里全是静态文件，随便丢到任意静态托管（Cloudflare Pages、宝塔、内网 IIS、NAS）。不需要运行环境。

**方案三：只在本地用**
双击 `dist/index.html` 就能看（动效会被浏览器规则关掉），或用上面那条 `http.server`。

## 备份

整个目录就是一个 git 仓库。想再加一层保险，每月把目录压一份存到网盘。

## 注意

- 这套样式**不含任何游戏素材**，颜色和形状都是自己做的一套 token，可以随便商用。
- 字体 Saira 是 SIL OFL 1.1 授权（`assets/Saira-OFL.txt`），自托管没问题；中文正文用系统自带的黑体，所以断网也能开。
- 公开的东西先假设所有人都会看到：单位全称、真实姓名、工号，写之前想清楚。

## 方块彩蛋（game/）怎么改

- 玩法代码在 `game/src/*.rs`（Rust + macroquad）。改完跑 `bash game/build-wasm.sh`（增量约 6 秒），
  产物自动落到 `static/game/`；再 `python3 build.py`、提交推送就上线。**CI 不需要 Rust**——
  仓库里存的就是编好的 wasm（`static/game/`），`build.py` 把 `static/` 原样拷进 `dist/`。
- 想确认它真能跑：`python3 gamecheck.py`（快，验五件事：WebGL2、`__MCL_BOOTED`、HUD 在跳、
  门面收没、点一下「改动」会不会 +1，最后一行打 `PASS 彩蛋能跑能挖` 才算过）。
  换手机宽度：`GAME_W=414 GAME_H=820 python3 gamecheck.py`。直接测线上：
  `GAME_URL=https://wkgn1.github.io/game/index.html?autoplay=1 python3 gamecheck.py`。
- 想彻底验：`python3 game/regress.py`（约 20 分钟，89 条断言，七组：桌面基础 / 静态 / 无 WebGL /
  看门狗 / 门面 / 手机触摸 / 浸泡）。它自己起本地服务、自己开无头 Firefox，测的是 `dist/`，
  所以要先 `bash game/build-wasm.sh && python3 build.py`。改 `loader.js` 之前先跑一次记下基线。
  它的判据怎么定的、有哪些坑（跨帧输入、俯仰灵敏度、无头报不出触屏）写在 `HANDOFF.md` 第十二节。
- 入口文案写死在两处：`templates/home.html`（首页那颗「方块彩蛋」按钮）、
  `templates/404.html`（右栏「藏了个东西」那行）。要去掉入口就删这两处。
- 地址是 `/game/`。地址后加 `?autoplay=1` 会自动开跑（不用点「点开玩」），截图和自动化都用它。
