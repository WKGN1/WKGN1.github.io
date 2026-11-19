# 交接说明（给下一个会话看的）

最后更新：2026-11-16 13:53（Asia/Shanghai）。上一轮会话把博客从设计、搭建到部署全部做完了，这一轮的任务是**测试、维护、调整**。

## 一、结论先记住

- 线上地址：**https://wkgn1.github.io**（已验证 200，子页面和资源都通）
- GitHub：账号 **WKGN1**，仓库 **WKGN1/WKGN1.github.io**（Public，分支 `main`）
- 发布方式：`git push` 到 `main` → GitHub Actions（`.github/workflows/deploy.yml`）自动 `python3 build.py` → 上传 `dist` → 发布 Pages。**1-2 分钟生效**，已验证两次成功。
- 站点风格：终末地风味，来自开源 skill `hhikr/endfield-ui-skill`（MIT 无授权限制的 CSS/token 体系，**不含游戏素材**）。强调色默认 `site` 预设（琥珀黄 + 暖纸 + 大切角）。
- 署名 **SuYzz**；一切与银行/柜面相关的内容都已删除。简介用的是「自己的机器，自己的模型，自己的记录」。

## 二、这台机器上的位置和凭据

- 项目目录：`/run/media/suyzz/e2270998-5ad9-4aef-860c-ba06f27ea0a9/strata/dsh/dsh_wechat/blog`
- 已有 git 仓库与历史（多次提交），`origin` = `git@github.com:WKGN1/WKGN1.github.io.git`
- 推送用 SSH 部署密钥：`~/.ssh/id_ed25519`（公钥已加进仓库 Deploy Keys，id 170013868，**有写权限**，实测能推）；`~/.ssh/config` 已配 `IdentityFile` + `IdentitiesOnly`，github.com 实际走 `ssh.github.com:443`（22 口不稳）
  **但 2026-11-16 收尾时这把密钥已失效**：用户按建议撤销了 PAT，而 GitHub 会连带删除「用该 token 添加的 SSH 密钥 / 部署密钥」，所以现在 `git@github.com` 报 `Permission denied (publickey)`。本机私钥还在（指纹 `SHA256:2sfcFuo8tBxIUqv8ea/NKMgOrHK7Ua34jUtrvICc4Co`），只差把它重新加进仓库。
- 推送命令示例：
  `GIT_SSH_COMMAND="ssh -o StrictHostKeyChecking=accept-new -o IdentitiesOnly=yes -i ~/.ssh/id_ed25519" git push origin main`
- token：那串 `ghp_...`（scope 只有 repo + workflow）**已让用户撤销**。要用 API（查构建日志、改 Pages 设置、加自定义域名）得再向用户要新 token。token 若拿到，存 `~/.config/blog/token`（chmod 600），别写进仓库。
  **规矩**：撤销 token 会连带删掉**用这个 token 添加的** SSH key / 部署密钥；在仓库界面里**手动**添加的部署密钥不受 token 撤销影响。所以若想要"以后可以随便撤 token"，先在仓库 UI 里手动加好部署密钥。

### 网络注意（重要，别踩）
这台机器到 `github.com` 的 **HTTPS 很不稳**（时好时坏，经常超时），但：
- `api.github.com` 稳定（0.3 秒级）→ 所有 REST 操作走它
- `github.com:22` / `ssh.github.com:443` SSH 可用 → **推代码只走 SSH，别用 https+token**
- 需要 curl 碰 `github.com` 时，用 `--resolve github.com:443:140.82.121.4` 之类钉 IP，并多重试

## 三、日常操作三条命令

```bash
cd <项目目录>
python3 build.py                              # 生成 dist/（零依赖，只用 Python 标准库）
python3 -m http.server -d dist 8080           # 本地预览 http://127.0.0.1:8080
git add -A && git commit -m "…" && <上面的 GIT_SSH_COMMAND> git push origin main   # 发布
```

另外 `python3 bundle.py` 会把已生成的页面打成**单文件离线预览**（`dist/预览-*.html`，字体/样式/脚本全内联），方便发给只能用手机的用户看效果。

## 四、改哪儿改什么

| 想改 | 改哪 |
| --- | --- |
| 站名、简介、座右铭、导航、联系方式、技能清单、强调风格、建站日期、域名、统计代码 | **只有一个文件：`site.json`** |
| 页面排布、字号、间距、响应式 | `assets/site.css` |
| 整套外观规则（颜色、切角、动效曲线、深浅色变量） | `assets/endfield.css` + `assets/tokens.json` |
| 某种页面的骨架 | `templates/home.html`（首页）、`templates/doc.html`（文章）、`templates/list.html`（列表）、`templates/about.html`（关于）、`templates/rail.html`（侧栏）、`templates/base.html`（公共头尾） |
| 文章内容 | `content/posts/*.md`；知识库条目 `content/notes/*.md`；关于页 `content/about.md` |
| 插图 | 放 `content/img/`，正文写 `![说明](img/文件名.png)`，构建自动拷贝 |

- Markdown 支持：`#`~`####` 标题、段落、`-`/`1.` 列表、`>` 引用、` ``` ` 代码块、`\|` 表格、`---`、行内 `` ` ``、`**粗体**`、`[链接](url)`、`![图](img/x.png)`。
- front matter 字段：`title` `date` `tags`（逗号分隔）`summary` `category` `draft`（`true` 则不出现在列表，但字数照算）。文件名以下划线开头（如 `_模板.md`）会被跳过。
- 页面上的数字（篇数、字数、阅读分钟、分类计数、"第 N 天"）都是 `build.py` 现算的真数字——**这套风格要求数字必须是真的**，别在页面里手写统计。
- 临时试效果：地址后加 `?theme=dark`、`?variant=coldstore`、`?noanim`（关动效，截图用）。深浅默认跟随系统，用户点"切换深浅"后记在 localStorage 的 `theme`。

## 五、已经踩过的坑（别再踩）

1. `bundle.py` 内联 JS 必须把 `</` 转义成 `<\/`——`variant.js` 注释里写着 `</script>`，否则 HTML 会提前闭合，整段 JS 变成页面上的文字。
2. `site.json` 里 `links` 是对象数组（`key`/`name`/`url`），别用解包迭代（曾把字典的键当值，页面上渲染出 `k=ng`）。
3. `build.py` 里所有页面都平铺在 `dist/` 根（文件名 `p-` 前缀=文章，`n-`=笔记），站内链接都是同级，因此 CSS/JS 路径永远是 `assets/…`；改成子目录要同步改 `{{ASSETS}}`。
4. 段落边界那个正则里不能同时出现两个捕获组 + 反向引用（Python 会报 "cannot refer to an open group"），已改成 `BLOCK_START` / `HR` 两个预编译表达式。
5. 没有自定义域名时**不要**生成空 `CNAME` 文件（已修）。要上自定义域名：`site.json` 的 `domain` 填上，构建会自动生成 `dist/CNAME`，再去 DNS 加 CNAME 记录。
6. `.gitignore` 忽略 `dist/`（Pages 由 Actions 现场构建），所以 `dist` 不在仓库里是正常的。
7. 已发布文章**别改文件名**（文件名就是 URL，改了旧链接失效）；改 `title` 字段无所谓。
8. 首页"最新"块取的是按日期排序后的第一篇；右栏标签只列出现次数前 8 个。
9. **模块 import 必须写相对路径**：`import … from 'assets/fx.js'` 是"裸说明符"，浏览器要求以 `./`、`../` 或 `/` 开头，否则整块 `<script type="module">` 直接 TypeError，fx.js 的 wiring（主题按钮、数字滚动）一条都不会跑，页面看着完全正常——**2026-11-16「切换深浅不生效」的真因就是这个**。模板里现在是 `'./{{ASSETS}}fx.js'`，别再改回去。
10. **JS 语法上限**：`assets/fx.js` ≤ ES2017、`assets/variant.js` ≤ ES2015、页面内联脚本 ≤ ES5。原因：微信安卓端内置浏览器是腾讯 X5 内核，很多机器上还停在 Chromium 77（iOS 微信是系统 Safari，老设备 < 13.1），`?.` / `??` / 无参数 `catch {}` 会让**整份文件在解析阶段报错**，于是 fx.js 全灭。门禁：`python3 check-es.py`（过了打 PASS）。
11. `templates/base.html` 里有一段**纯 ES5 的主题兜底**（按钮上打 `data-theme-fallback=1`）：fx.js 跑成功会给按钮写 `aria-pressed`，兜底一看就不插手，圆形展开动画仍归 fx.js。改主题相关代码前先想清楚这条链，别把两边都绑上点击（绑两次 = 每点一下就翻回来，看着像坏了）。
12. 凡是用 `document.startViewTransition` 的地方：在**有** VT 的浏览器里回调是被推迟执行的，同步读 DOM 什么都看不到。`themetest.py` 的做法是要么把 `startViewTransition` 置成不存在（老浏览器的真实路径），要么换一个立刻执行回调的假货。

## 六、当前内容和线上状态

- 文章 2 篇：`这台博客是怎么搭起来的`（也当写法模板）、`为什么在自己的机器上跑模型`
- 知识库 2 条：`机器跑不动时的排查顺序`（分类 运维）、`模型量化怎么选`（分类 本地模型）
- 关于页：我是谁 / 我在做什么 / 这个站怎么用 / 联系我
- 合计约 700 字，`since` 设的 2026-11-01，所以首页显示"第 15 天"这类真数字
- 联系方式：GitHub 已指向真实账号 `github.com/WKGN1`；邮箱是真的 **`suyyz@foxmail.com`**（2026-11-16 用户给的，只出现在关于页的「联系我」）；微信那行写的是"可私信"
- 仓库是 Public：源码和用户写的文章全球可见（免费 Pages 的限制，用户已接受）

## 七、待办 / 用户可能让你做的事

1. 换真实邮箱 —— **已做完**（`suyyz@foxmail.com`，写在 `site.json` 的 links 里）
2. 那两篇目前偏"说明文"，用户可能想换成自己的真内容；想删就删（记得同时看要不要留 `_模板.md`）
3. favicon + 社交分享卡片图（写进 `site.json` 的 `head_extra`，图片放 `assets/` 或 `content/img/`）
4. 访问统计（国内能用的小统计，一段代码进 `head_extra`）
5. 自己的域名（买域名 → `site.json` 的 `domain` → DNS CNAME）
6. 站内搜索（知识库变大后再加；纯静态可用一个生成 JSON 索引 + 前端过滤的小方案）
7. 配色/深浅的最终确认：当前默认 `variant=site`（琥珀），备选 `standard` / `coldstore` / `nightshift`
8. 想加"能力优势/作品集"更重的板块（比如项目卡片、履历时间线）——注意这套风格的规矩：唯一强调色、切角只在可点击元素上、1px 细线分区、不许发光渐变卡片、数字必须真实

## 八、和这位用户打交道的规矩

- 他只在**微信**上、用手机，看不到也打不开这台电脑的目录：**不要让他"去打开某个路径"**。要给他看东西就直接发——图片当照片发，其它文件当附件发（用 `send_to_wechat`，绝对路径）。
- 回复要**短**，适合手机读；能用三行说完就别写十行。
- 他打不开 `*.lhr.life` 这类境外临时域名（微信内置浏览器/网络都会挡），要预览就发**截图照片**或**单文件 HTML 附件**（附件要告诉他"右上角用浏览器打开"）。
- 涉及公开内容的隐私提醒：站点和仓库都是公开的，单位名、真名、工号、客户数据这类东西写之前要提醒他。
- 需要他做 GitHub 上的操作时，给精确到"打开哪个网址、点哪个按钮"的步骤（他愿意照做，但讨厌自己找）。

## 九、上次会话留下的进程

- 本机预览服务：`python3 -m http.server 8088`（静态服务 `dist/`）可能还在跑，端口 8088
- 临时外网通道：`ssh -R 80:127.0.0.1:8088 nokey@localhost.run` 的自动重连循环，日志 `/tmp/tunnel.log`、当前域名记录在 `/tmp/tunnel_url.txt`（上次的域名 `88dbd21479f949.app.lhr.life`，现已不需要，线上走 github.io）。要重开：
  `setsid bash -c 'while :; do ssh -n -T -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ServerAliveInterval=25 -o ExitOnForwardFailure=yes -R 80:127.0.0.1:8088 nokey@localhost.run >>/tmp/tunnel.log 2>&1; sleep 8; done' &`
  注意：`pkill -f` 的匹配串如果出现在你自己的命令里会把自己杀掉（上一轮自杀过两次）——找进程用 `pgrep -x ssh`，杀进程用 pid 文件或 `pgrep -x`。
- 截图/预览产物放在 `/home/suyzz/blogshot/`（线上截图 `live-grid.png`、单文件预览 `预览-*.html` 等）。Firefox 无头截图要点：snap 版 Firefox 只能往 `~/snap/firefox/common/` 里写文件，命令要带 `--no-remote --profile /home/suyzz/snap/firefox/common/ffprof`。

## 十、下次开工先跑这几步（自检）

```bash
cd <项目目录> && git status && git log --oneline -3
python3 build.py                       # 应该报「生成完成：N 个页面…」
python3 -m http.server -d dist 8080 &  # 或直接用线上地址
curl -s -o /dev/null -w "%{http_code}\n" https://wkgn1.github.io/    # 期望 200
```
改完之后：提交 → SSH 推送 → 等 Actions（`GET api.github.com/repos/WKGN1/WKGN1.github.io/actions/runs?per_page=1`，没 token 就只能靠用户或让用户再给一个）→ curl 线上确认 200 → 给用户发一张截图照片。

## 十一、本轮收尾时留下的事（下一个会话请先处理）

1. **推送权限需要恢复**（**2026-11-16 15:00 已解决**：用户在仓库 UI 手动加了部署密钥，Title `blog`，勾了 write；实测 `git push origin main` 成功、Actions #4 success。以后撤销 token 不会再带走这把钥匙）：
   - 办法 1（推荐，之后不依赖 token）：用户打开 `https://github.com/WKGN1/WKGN1.github.io/settings/keys` → **Add deploy key** → Title 填 `blog`，Key 粘贴这串公钥 → **务必勾上 Allow write access** → Add key：

     `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMqAlC683hEwNXNDkBT+k/c0pTM0/jrwL/xLJ9QvwdBd blog-deploy`

     加完即可推送：

     `GIT_SSH_COMMAND="ssh -o StrictHostKeyChecking=accept-new -o IdentitiesOnly=yes -i ~/.ssh/id_ed25519" git push origin main`

   - 办法 2（最省事）：让用户再发一串 token（classic，勾 repo + workflow），用它 POST `/repos/WKGN1/WKGN1.github.io/keys`（body 里带 `"admin": true`）把上面这串公钥加回去；**提醒他这串 token 先别撤销**，否则又会把密钥一起删掉。
2. **有一个提交没推上去**（**已解决**：`444439a` 与 `1f6e9f6` 已于 15:00 推送，Actions 运行 #4 success，远端 HEAD 现为 `1f6e9f6`）。
3. **交接包**：`博客交接-20261116.zip` —— 含全部源码 + `.git`（完整历史与 remote 配置），不含 `dist/`（用 `python3 build.py` 现场生成）。解压到任意目录，`cd` 进去干活。
4. 用户只想看效果时直接给线上地址：`https://wkgn1.github.io`（深色 `?theme=dark`，换配色 `?variant=coldstore`）。要发照片就用本机 Firefox 无头截图（snap 版 Firefox 只能往 `~/snap/firefox/common/` 里写文件，且要带 `--no-remote --profile /home/suyzz/snap/firefox/common/ffprof`）。

## 十二、2026-11-16 15:00 自检结果（本轮实测，不是推测）

- 线上正常：`/`、`posts.html`、`about.html`、`kb.html`、`sitemap.xml`、中文文件名文章页（percent-encode 后）全 200；`assets/site.css` 200。
- 本地 `python3 build.py` 正常：8 页面 / 文章 2 / 笔记 2 / 717 字。
- 截图实拍（浅色、深色、400px 窄屏）排版都正常：合成图 `线上自检-20261116.png` 在 `/home/suyzz/blogshot/`（截图临时文件在 `~/snap/firefox/common/tmp/`）。
- **实测缺失（给用户列过的待办依据）**：`favicon.ico` 404、`robots.txt` 404、`feed.xml`/`rss.xml` 404、`404.html` 404（走 GitHub 默认页）；`site.json` 的 `head_extra` 为空 → 无统计、无 favicon、无分享图；页面 head 里没有任何 `og:`/`twitter:` 标签 → 分享到微信/QQ 没有卡片。
- 内容侧待确认：`site.json` 的 links 里邮箱还是 `示例@example.com`（公开可见）；`content/img/` 是空的（文章零插图）。
- 性能待办：`assets/Saira.ttf` 476KB 整包下发（`font-display: swap` 已有，中文走系统字体不受影响）；可切 woff2 + 拉丁子集，约省 400KB。
- 顺带可清理：`assets/tokens.json`（12KB）构建后运行时不需要（`variant.js` 里只是注释提到），目前仍被发布到线上。
- 遗留进程：`python3 -m http.server 8088 --bind 0.0.0.0 --directory <项目>/dist` 仍在跑（局域网可访问，pid 当时是 400176）；外网 localhost.run 隧道已断，不再需要。
- 备份：`/home/suyzz/blogshot/博客交接-20261116.zip`（含 `.git` 完整历史，不含 `dist/`）。
- 版本一致性：本机 Python 3.14.4，CI 固定 3.11（本机没有 3.11，改动后按 3.11 兼容写法来过一遍再推）。


## 十三、2026-11-16 15:55 修好「切换深浅不生效」

- 现象：用户报线上「切换深浅」按钮点了没反应。
- 真因（已在本地复现，有报错原文）：`dist/index.html` 里是 `import … from 'assets/fx.js'`，裸说明符 → `TypeError: "assets/fx.js" 是一个裸说明符，但未重新进行任何映射` → 整块模块脚本没执行 → `wireTheme()` 从没绑过点击。CSS、variant.js、`?theme=dark` 都不受影响，所以只有按钮像坏了。
- 改动（三处，都在源码里）：
  1. `templates/base.html`：import 改成 `'./{{ASSETS}}fx.js'`。
  2. `templates/base.html`：加了一段纯 ES5 的主题兜底脚本（见坑 11），fx.js 挂了按钮照样能用。
  3. `assets/fx.js` 去掉 `?.` / `??` / 无参 `catch`（降到 ES2017），`assets/variant.js` 降到 ES2015 —— 让 X5 那种老内核至少能解析 fx.js。
- 新增两个仓库内工具（都只用标准库）：
  - `python3 check-es.py`：JS 语法门禁（调 npx 里的 acorn，第一次要联网）。
  - `python3 themetest.py dist`：往 `dist/` 塞 `t-nofx.html`（只剩兜底）、`t-novt.html`（fx.js，无 VT）、`t-vt.html`（fx.js，圆形展开分支），用 `firefox --no-remote --profile ~/snap/firefox/common/ffprof --screenshot=… --window-size=1280,760 "http://127.0.0.1:8088/t-novt.html?noanim"` 截图看绿/红横幅；单文件预览版也照这个办法验（把 `TEST`+`BREADCRUMB` 贴到 `预览-首页.html` 的 `</body>` 前）。
- 本轮自检结果：4 种组合全绿 —— ① 抽掉 fx.js 只剩兜底 ✅ ② fx.js 且无 VT ✅ ③ fx.js 走 VT 分支 ✅ ④ 单文件离线预览 ✅（都是点一下变深色、再点回浅色、token `--ef-paper` 跟着翻、`aria-pressed` 同步、`localStorage.theme` 有存）。
- 结论图：`/home/suyzz/blogshot/主题切换自检-20261116.png`（已发用户微信）。`t-*.html` 只在 `dist/` 里，不进仓库（`dist/` 被 gitignore）。

## 十四、2026-11-16 16:20 用户新报：大数字那一行的动效

- 用户原话：**「显示 003 大数字那一条的动效不对，文字不能一起震动」**。这条投诉很可能是 fx.js 修好之后才第一次露出来的（之前整块 fx.js 没执行，滚动效果根本没跑过）。
- 已实测排除"宽度把正文挤来挤去"这个猜测：`themetest.py` 生成的 `t-nojit.html` 里把大数字逐个换成 000…999 并量宽度和正文左边界 —— **数字宽 282.2→282.2（差 0px）、正文左边抖 0px**，Saira 的数字本来就是等宽的；顶部那几个计数（15 / 4 / 2 / 717）自身宽差也是 0px。所以正文没有被挤动，问题在数字自己滚动的观感上。
- 涉及代码：`templates/home.html` 的 `.latest` 块（`data-fx-host` + 大数字上的 `data-fx`，鼠标进入/获得焦点时 `fx.js` 的 `wireReadouts` 会让 `data-fx` 的数字滚一遍，逐位从左到右锁死，600ms）；观感细节在 `assets/fx.js` 的 `scramble()`。
- 2026-11-16 16:22 状态：已把三种改法发给用户选，等他回哪个（不滚 / 只滚末位快一点、前面的立刻锁住 / 整块文字也别带滚动）。**改之前别自己猜着动 fx.js**。

## 十五、2026-11-16 20:40 「003 那条文字跟着抖」查清并修好

- 用户澄清：不是数字好不好看的问题，是**数字后面的正文位置被拽着动**，抖得看不清。
- 真因（定格实测出来的，不是猜的）：数字滚动时随机字形宽度不一致，而这一栏是 CSS Grid 的 `auto` 轨道，宽度跟着字形走 → 右边正文整段被推走。最容易犯的时机是 **Saira 字体还没下载完、先用系统字体顶着**（系统字体的数字不等宽），`font-variant-numeric: tabular-nums` 对没有 tnum 特性的字体不起作用。
  - 修复前实测：数字宽 282.2→312（差 29.8px），正文左边 386.2→416（**抖 29.8px**）。
- 改动：
  1. `assets/site.css`：`[data-fx], [data-fx-load] { font-variant-numeric: tabular-nums lining-nums; }`，并且把大数字那一栏**按位数钉死**——`.latest .ef-hollow-no { min-width: 3ch; max-width: 3ch; text-align: center; }`（编号是 build.py 的 `%03d`，永远三位，所以 3ch 安全）。宽度从此不跟字形走。
  2. `templates/base.html`：加 `<link rel="preload" as="font" crossorigin href="{{ASSETS}}Saira.ttf">`，让字体早点下载，别在滚动滚到一半的时候才换字体（换了 `ch` 会重算，会抖最后那一下）。
  3. `assets/fx.js`：`scramble()` 里加了宽度占位 `pinWidth()/pinRelease()`（量一次最终宽度，滚动期间用内联 min/max-width 锁死，滚完释放）——单测证明函数本身有效（克隆上调 `scramble()` 后内联 `min=max=360px`）。
  4. `bundle.py`：单文件预览里把这条 preload 去掉（预览里字体已经内联成 data:，留着只是白报一个 404）。
- 修复后实测（同一套定格法，9 帧）：**数字宽 384.95→385（差 0.05px），正文抖 0.05px**；GIF 九帧里右列正文的起始位置一帧都没挪。字体状态 `Saira 可用=true`（preload 起效）。29.8px → 0.05px。
- 还没查清的一件事（诚实记着）：hover 触发的真实滚动路径上，逐帧读 `realNo.style.minWidth` 是空的（占位没挂上），但同一份 fx.js 里直接调 `scramble()` 是会挂上的。目前宽度稳定是靠 CSS 的 `3ch` + preload 做到的，占位函数算第三层保险。要接着深挖就从 `wireReadouts` 的 `run()` 那条链查。
- 工具：`/home/suyzz/blogshot/filmgif.py` —— 把 fx.js 真跑起来、按 t=0/60/…/600ms 逐帧定格成 GIF，同时在页面顶部打印"数字宽 / 正文位移 / 每帧字符 / 字体就绪状态 / 占位内联值"。要点：**每次用全新 Firefox profile**，否则 `assets/*.js|css` 会被缓存住，改动看着像没生效（这次就这么被骗过一回）。
- 规矩：**加任何"数字滚动/计数器"类效果，先占位再滚**（`ch` 或内联锁宽），并且配套 `python3 check-es.py` 与 filmgif 实测；别在只改 CSS 的情况下宣称"不抖了"。

## 十六、2026-11-17 01:30 补齐基础件：图标 / 404 页 / robots / RSS / 分享卡片，并把 hover 撞车查到底

### 做了什么（build.py 还是零依赖，新工具只在改动它们自己时才跑）

1. **favicon**：形状是"右上角切角的深色方块 + 三条琥珀横杠 + 一个小方块"，站内"记录清单"那几行的压缩版（深色底是为了在浅色和深色标签栏都看得见）。
   - 生成器：`python3 favicon.py`（**要 Pillow，build.py 不用它**）→ `assets/favicon.svg`、`assets/favicon-32.png`、`assets/apple-touch-icon.png`、`static/favicon.ico`。
   - 要换样子只改 favicon.py 里的 `BADGE / BARS / TAG / FIELD` 和两个颜色再重跑，**别手改生成物**（SVG 和 PNG 都由那一份几何出来）。
   - 新规矩：**`static/` 里的文件原样进 `dist/` 根**（favicon.ico 这类"客户端直接去根上取"的文件放这里，爬虫不读 `<link>` 也能拿到）。
2. **404 页**：`build.py` 的 `not_found()` + `templates/404.html`。这一页可能在**任意深层地址**下被打开，所以资源和链接一律从站点根走（`/assets/…`、`/posts.html`）：靠 `page(..., root_rel=True)` 和 `rail(..., root_rel=True)`。以后再加"会在深层路径下被服务"的页面，同样处理，否则 CSS 会 404 成一张白板。
3. **robots.txt**（`robots_txt()`）、**feed.xml**（`rss_feed()`：RSS 2.0；草稿不进订阅；description 是纯文本（先把标签去掉再把实体还原，别把 `&amp;` 二次转义）；分类和标签重名要去过再发；时间带 +0800）。
4. **head 里加的东西**（都在 `templates/base.html`，值由 `page()` 填）：图标三条、RSS 声明、`canonical`、`og:`/`twitter:`（含 1200×630 的 `og:image`），文章页另有 `article:published_time` / `article:tag`。首页的 canonical/og:url 就是站点根，**不挂 index.html**（同一页两个地址会被当重复内容）。地址一律 `abs_url()` 走 percent-encode（文章名是中文）。
5. **分享卡片**：画板 `templates/sharecard.html`（**不发布**，只有工具用它）+ `python3 sharecard.py` → `assets/share-card.png`。默认深色，`--theme light` 出浅色版；也可以在 `site.json` 里写 `card_theme`。**卡片页必须带 `variant.js`**，否则强调色退回默认正黄而不是琥珀（第一版就是这个错）。卡片上的数字来自 build 现算，别写死。
6. `assets/tokens.json` **不再发布**（运行时没人读，12KB）：`build.py` 里的 `NO_PUBLISH = {"tokens.json"}`。
7. sitemap：草稿不再进去，地址 percent-encode。
8. `bundle.py`：离线单文件预览里把图标 / apple-touch / RSS 声明的链接剥掉（和之前剥字体 preload 同理——包里没这些文件，留着只是白抓一次 404）。

### hover 撞车（第 14 节那条投诉的延续，这次查到底了）

- 现象：鼠标进"最新"那一行，右边的正文被整段拽着走。
- 真因（逐帧定格实测，不是猜的）：`wireReadouts` 开场那串 `setTimeout` 没留住 handle，hover 时 `run()` 又把开场滚动放了一遍（所以 003 会"滚完又回头滚"）；而 `cancelAnimationFrame` 之后的状态是"正在跑但没有占位宽度"——占位要到下一次 `scramble()` 才挂上，于是这一栏宽度先跟着随机字形乱缩（实测 **-34px**：120.5 → 86.5），正文被推过去 26.3px。fx.js 修好之前整块模块没执行，这条从来没露过头。
- 修法：`run()` 里**先 `clearTimeout` 掉开场 timer（并清零）再滚**。`pinWidth()` 的测量**保留**（实测滚之前 measured==rendered：86.5→86.5，`pinW/pinInline` 也确实是同一个值），CSS 的 `min/max-width:3ch` **也别删**（Saira 加载失败时 `ch` 是唯一防线）。
- 实测：修完 hover 帧宽 84.4 / 90.3 / 113.7 / **120.5** / 120.5，全程不回缩，每帧 `pinInline` 都挂着；`check-es.py` PASS。证据 GIF：`/home/suyzz/blogshot/dig003/hover-滚动撞车-修复前修复后对比.gif`（还有一张 `hover-真实滚动帧.gif`）。

### 新工具：以后要截图就用 `wdshot.py`

- `python3 wdshot.py <URL> <输出.png> [宽 高]`：geckodriver + **每次全新 profile** + 等 `document.fonts.status=loaded` + 截之前核对 `innerWidth/innerHeight` 和 PNG 头里的宽高，不到位就打「没到位」并返回 2。`sharecard.py` 就是调它。
- **别再用 `firefox --screenshot`**：它只等 load 事件（字体可能还在路上，量出来的宽度是系统字体顶的假值），而且这台机器上它会**卡死**（上一轮那个 180 秒没返回的就是它）。
- 实测的怪事（别再查一遍）：这台机器上 headless Firefox **改不动内容尺寸**——`POST /session/…/window` 用真窗口 handle 返回成功，但 innerWidth/innerHeight 一动不动（handle 填 root/parent/window 都是 404 no such window）；窗口高度比内容多 **86px**（开 630 → 内容 544，开 900 → 814）。wdshot 的办法：先按目标开 → 量差值 → 把差值加到 `-width/-height` 上重开一个会话（实测第二次就正）。
- geckodriver 新版 `POST /window` **必须带 `handle` 字段**（不带就 400 missing field handle）。

### 又踩到的坑（都记下来）

- `pkill -f "http.server -d dist 8093"` **又把自己杀了**（匹配串出现在我自己那条命令里）。规矩：找进程 `pgrep -x`，杀进程用 pid / pid 文件，别用 `-f`。
- Python 里 **`SystemExit` 不是 `Exception` 的派生类**：`finally` 里 `except Exception` 拦不住它；而收尾再抛一句会**顶掉正文真正的失败原因**（当时屏幕只剩 "DELETE /session/ -> HTTP 405"，白查一轮）。收尾一律 `except BaseException`，并且先判 sid 空不空。
- 门禁 `check-es.py` 偶尔第一遍报"没过"：npx acorn 要走网络，网络抖一下就被当成语法错。**同一改动重跑一遍再下结论**（这次第二遍就 PASS）。
- robots.txt 的 Sitemap 行我第一版写成 `…/sitemap.xml/sitemap.xml`（`abs_url()` 已经带文件名了，别再加一层）。
- **端口 8085 是用户自己的 `mi50-gateway` 服务，别动别杀。**

### 线上待验证（推完之后下一轮先确认这几条）

`/404.html` 和任意不存在的地址是不是出这张自定义页（GitHub Pages 用户站支持，但要实测）、`/favicon.ico`、`/assets/share-card.png`、`/assets/favicon.svg`、`/feed.xml`、`/robots.txt` 是不是 200，文章页 head 里 og 齐不齐，`assets/tokens.json` 是不是真的不再上线。

### 还没定的两件事

1. 卡片深浅：**深色那张已经作为默认提交**，浅色对比图已发用户微信，等他说要不要换。
2. 两篇说明文换成他自己的真内容（等他给素材）。

## 十七、2026-11-17 07:30 内容换成他自己的真东西（第一篇真数字的文章上线）

用户选的方向：**"我根据你真在跑的东西先起草，你再改"**。卡片配色他选了**深色**（就是已上线那张，不用动）；统计/域名/字体瘦身这轮**都不加**。

### 这轮写的两篇

1. **新文章 No.005** `content/posts/2026-11-17-32G显存怎么跑71G的模型.md`：MI50 上跑 MoE 的三层账 + 一次请求的实测 + "一卡两门"的搭法。
2. **重写 No.002**（文件名/地址没动，只换正文，里面写了一句"11-17 重写"）：三条理由三条代价全部换成这台机器上真发生过的。

### 文章里每个数的出处（用户要核的时候按这个查）

| 文里的说法 | 从哪量的 |
| --- | --- |
| Xeon E5-2698 v3 / 32 线程 / 内存 61 GiB / Ubuntu 26.04 | `lscpu`、`free -g`、`/etc/os-release` |
| MI50 = Vega 20、显存 32 GiB | `lspci`、`/sys/class/drm/card1/device/mem_info_vram_total` = 34342961152 |
| 模型 76,142,283,328 字节（≈71 GiB） | `work/models/…` 两个 GGUF 分片相加（44.09 + 26.82 GiB） |
| 就绪 975 秒 | 实例 JSON 里的 `engine_ready_s` |
| 43.1 tok/s、prompt 192115=189385复用+2730现读、草稿 599/679 | 引擎日志 `.local/share/strata/install/strata-swift-iq3_xxs.log` |
| 一次请求专家读 46427.1 MB、其中 46421.3 MB 在页缓存、真读盘 91.5 MB | 同一行日志的 "file tier I/O" |
| 会话缓存 6 GiB、显存预留 1280 MiB | systemd unit 的 `STRATA_OVERRIDE=--conversation-cache-mib 6144` 与实例 JSON 的 `--vram-reserve-mib 1280` |
| 40.2 / 49.2 tok/s、冷启动 1.9 tok/s | 同日日志里另外几行 |
| TDP 300 W / 145 W、整机四百瓦上下 | 公开规格 + 明确写了"没上功率计" |

**替他留着没写的（他觉得可以实名再加回去）**：模型名和别名（`swift-1.5-iq3_xxs`、目录名 `sc117`、那串 `Qwen3.8-Flash-Next-GSQ-RCO-abliterated-IQ3_XXS` 文件名）、引擎和桌面端的名字（Strata / dsh）、端口以外的地址、API key 和 `~/.config/mi50-gateway/lan-key`（**key 一律不上站**）。文里统一写成"我自己的引擎 / 桌面端 / 命令行端"。

### 这台机器上相关的一堆事实（下一轮要用就别再量一遍）

- 脚本都在 `~/.local/bin/`：`mi50-gateway`（8085→18500，把每次加载换的 key 补上；`0.0.0.0` 监听，外面来的一律要 lan-key）、`strata-mi50-supervisor`（桌面端开着就停独立引擎、起网关；关了反过来；5 秒一轮）、`strata-mi50-autoload`（每 10 秒看桌面端 API，没实例没任务才发 load）、`wait-for-mi50`（在 KFD 拓扑里等 `gfx_target_version 90006`，2 秒×90 次，等不到也放行）、`strata-up`。
- systemd **用户**服务：`strata-mi50-gateway.service`、`strata-mi50-supervisor.service`（都 `Restart=always`）、`strata-iq2xs.service`（oneshot + `TimeoutStartSec=420`，**现在状态是 failed**，那是另一套 IQ2_XS 的配置，不是当前在跑的那条路）。
- 桌面端的宿主 API 在 `127.0.0.1:47831`（`/api/native`、`/api/native/operations`），引擎在 `127.0.0.1:18500`，网关在 `0.0.0.0:8085`。**8085 是用户自己的服务，别杀别动。**
- 磁盘：NVMe 825G 用了 114G，模型和 pack 在 `work/` 下（模型 72G、pack 1.5G、MTP 草稿 787M）。

### 还没定的

1. 名字要不要实名（上面那张"替他留着没写"的清单）——已问用户。
2. **No.003《这台博客是怎么搭起来的》要不要留**：它现在兼作写法模板的示范；要退掉的话记得 `content/posts/_模板.md` 还没有，需要就先建一个。
3. 两条知识库条目（量化怎么选 / 排查顺序）也是说明文口吻，可以按 No.005 的路子换成真数字版——**这轮没动**，等用户发话。

### 顺带修的一个真错：CI 上的"今天"少一天

GitHub runner 在 UTC，`build.py` 原来用 `date.today()`。北京时间早上 8 点前发布，首页那个 `<time>` 和"第 N 天"就会少一天——2026-11-17 07:5x 实测过：线上是 `2026.11.16 / 第 15 天`，本地是 `2026.11.17 / 第 16 天`。改成 `build.py` 顶部的 `today()`（按 CST，和 RSS 时间同一个来源）。**别把 `date.today()` 直接写回去。**

顺带核对过字数：`wordcount()` 是"汉字按字计 + 拉丁按词计"，文章页、列表页、分类账、总数用的是同一个函数，2026-11-17 实测一致（No.005 986 字、No.002 537 字、No.003 232 字、两条笔记 152 / 156，合计 2,063）。**别在不同地方各写一套字数算法。**

### 11-17 上午用户的三个决定，和照它们做的事

1. **名字全都写实名**：模型 `swift-1.5-iq3_xxs`、文件全名 `Qwen3.8-Flash-Next-GSQ-RCO-abliterated-IQ3_XXS-00001-of-00002.gguf`（47,342,144,896 字节）+ `-00002-of-00002`（28,800,138,432）；引擎 **Strata**（`strata --version` 打 **0.1.59**，gfx906 那版自编，旧版留在 `strata.bak-0.1.48-20261111-…`）；桌面端 `Strata-0.2.11.AppImage`；命令行端 **dsh**。API key 和 `~/.config/mi50-gateway/lan-key` 仍然一律不上站。
2. **No.003 重写成真记录**（文件名/地址没动）：现在是真实数字表 + 三个 bug 的实测数据，仍然顺手把写法全演示一遍。写法模板是 `content/_模板.md`——**它早就存在**（下划线开头 + `draft: true`，生成时跳过，不上线也不进 RSS），别再"以为要新建"。
3. **两条知识库条目也换成真数字版**：No.001 量化怎么选（真尺寸 + "只有 IQ3_XXS 一个版本，所以精度差没实测"这条写明了）、No.004 排查顺序（六步，每步一条真命令）。

字数从 2,063 涨到 **3,264**。线上逐页核对过：No.005 1,012 / No.003 757 / No.002 539 / 笔记 437 + 519，五个数相加正好 3,264，和首页那个总数一致。

### 这一轮新量到的数（下一轮要用别再量一遍）

- `strata --version` = **0.1.59**；桌面端 AppImage 版本 **0.2.11**；备用小模型 `qwen3-0.6b-q4.gguf` = 396,705,472 字节。
- `systemctl --user status strata-iq2xs.service` 的 journal：`Main process exited, code=killed, status=15/TERM`、"31.877 秒 wall clock 只吃 4.246 秒 CPU"。**教训：`Type=oneshot` + `RemainAfterExit` 的服务被监督进程 `stop` 之后 unit 会显示 failed，那不是崩了，别当故障追。**
- 构建一次 **0.07–0.08 秒**（量三次）；Actions 从 push 到跑完 **23–29 秒**（六次实测），**跑完 8 秒后 curl 就已经是新内容**；`dist/` 8.6 MB，其中 `Saira.ttf` 476 KB 占大头（还没瘦身）。
- 生成速度当天另外几条：43.4 / 40.9 tok/s（加上之前的 43.1 / 40.2 / 49.2）。

### wdshot.py 的一处放宽（踩过就补）

`document.fonts.status` 在某些页面上会一直挂在 `loading`，害得截图白报"没到位"。改成 `font_ready()`：status=loaded **或** `document.fonts.check('16px "Saira Variable"')` 为真都算到位，轮询从 10 秒加到 15 秒。同一张页面之前报"没到位"、现在 OK（实测过）。

### 又踩一次的 shell 坑（两次了，写下来）

`cd 目录 && nohup 命令 &` 里的 `&` 会把**整条** `A && B && C` 丢进后台子 shell：`cd` 只在子 shell 里生效，`$!` 也是那个子 shell 的 pid。后果有两个——后面的命令在**原来的目录**里跑（我因此找不到 `wdshot.py`），以及 `kill $!` 杀不掉里面那个 `http.server`（结果 8093 / 8095 各漏了一个服务在监听）。**规矩：要么用工具的 workdir 参数指定目录，要么 `cd 目录;`（分号）再 `nohup … &`；收尾时按 pid 验一遍 `ss -ltnp | grep 809`，别信 echo 出来的"服务已停"。**

## 十二、方块彩蛋（2026-11-18 这轮做完并修好，已上线）

上一轮把 Rust 小游戏写完、wasm 也编好了，但没提交、没上线、而且**其实跑不起来**（没人真验过）。这轮验了、修了、上线了。

### 东西在哪、怎么动

- 源码：`blog/game/src/*.rs`（Rust + macroquad 0.4，9 个文件约 3,300 行）。
- 编 wasm：`cd blog/game && bash build-wasm.sh`（增量约 6 秒）。工具链在 `dsh_wechat/.cargo` 和 `dsh_wechat/.rustup`，脚本自己往上层找；`wasm32-unknown-unknown` 目标已装。产物自动拷进 `blog/static/game/`，wasm 文件名带内容哈希（防 CDN 吃旧版），`manifest.json` 里记着叫什么。
- **CI 不需要 cargo**：`static/game/` 里存的就是编好的产物，`build.py` 把 `static/` 原样拷进 `dist/`（第 472-479 行那段）。改了 Rust 必须本机重编再提交，CI 不会编。
- 自检：`python3 blog/gamecheck.py [输出png]`。**最后一行打 `PASS 彩蛋能跑能挖` 才算过**，它验五件事：WebGL2 在不在、`__MCL_BOOTED` 有没有置位、HUD 有没有在跳、门面收没收、往画布中心点一下 HUD「改动」会不会 +1（不 +1 = 输入没接到 wasm）。`GAME_W=414 GAME_H=820` 换窗口宽度；`GAME_URL=https://wkgn1.github.io/game/index.html?autoplay=1` 直接测线上。截图默认写 `/home/suyzz/blogshot/game-check.png`。
- 入口（写死的，要挪就改这两处）：`templates/home.html` 里 hero 的「方块彩蛋」按钮、`templates/404.html` 右栏「藏了个东西」那一行。sitemap 里有 `game/index.html`，游戏页自己有 `rel="canonical" href="/game/"`（免得 `/game/` 和 `/game/index.html` 算两个地址）。

### 这轮修掉的四个真 bug（每一个都会让彩蛋变成一具尸体）

1. **wasm 名字定早了**：`loader.js` 的 `startGame()` 在读 `manifest.json` 之前就把 `url` 定成 `manifest.wasm || 'mc-lite.wasm'`，那时 `manifest` 还是 null → 永远请求兜底的 `mc-lite.wasm` → 404（文件名带哈希）。现在 url 挪到 manifest 到手之后再定。
2. **macroquad 0.4.16 的保留贴图槽名**：`MaterialParams.textures` 里列 `"Texture"` 会直接 panic（`quad_gl.rs`："you can't use name `Texture` for your texture…"）。着色器里照写 `uniform sampler2D Texture;` 没问题，但**贴图要挂在 `Mesh.texture` 上**（`game.atlas` 存一份，建网格时 `texture: self.atlas.clone()`），每次 draw 自动占那个保留槽。图集 `Texture2D` 的 `Clone` 只是句柄拷贝，36 个网格各拿一份不贵。
3. **`hidden` 属性被 CSS 顶掉**：`.g-gate`、`.g-hud` 都是 `display:flex`，作者样式会盖过 HTML `hidden` 的 UA 样式 → JS 收了半天画面上还盖着门面。`game.css` 现在有一条 `[hidden] { display: none !important; }`，这个页面所有靠 `hidden` 切换的面板都靠它。
4. **没人置 `__MCL_BOOTED`**：`watchBoot()` 一直等这个变量，可它只在 HTML 里被置成 false → 门face 永不收、HUD 永不亮。现在 Rust 从 `Waiting` 进 `Running` 那一帧调 `mcl_booted()`。另外加了 `mcl_log()` + panic 钩子：**崩了会把"崩在哪一行、为什么"写进 console 和页面底部**（以前只会显示一句误导性的"这台机器的浏览器跑不了 wasm"——上一轮就是被这句话骗到的）。

### 无头验证的要点（别再重新摸索）

- 地址要带 `?autoplay=1`，不然脚本得去点「点开玩」按钮。
- geckodriver 的 `POST /session/:id/se/log` 返回 **405**，拿不到浏览器日志；要查 wasm 里的事，靠页面早期挂 `window.addEventListener('error', …, true)` + `unhandledrejection` + 自己包一层 `console.*`（`/tmp/gdiag3.py` 是这个套路，没进仓库）。
- 本机 headless Firefox 149 有 **webgl2**（软件渲染），所以这台机器能测；帧率 58–60。
- 起步要 7–10 秒（wasm 下载 + 编译），所以"等 2.5 秒没动静就亮兜底文案"那个定时器会抢跑 —— 真跑起来之后必须把兜底收掉（`watchBoot` 里已加）。

### 实测数字（2026-11-18 本机）

- wasm 本体 **524,633 字节**，gzip **176,754** 字节（brotli 没装 brotli 命令，没量）。
- 从打开到 HUD 亮：7–10 秒；HUD 帧率 58–60；`改动` 计数从 0 → 1（点一下真挖掉了）；存档写 IndexedDB 正常，HUD 上显示"已存档"。
- 414 宽（手机宽度，headless 实际给了 500 宽）也能跑能挖，同一套结论。

### 还没做 / 可以考虑

1. **手机端触摸**：挖 / 放 / 摇杆走路 / 双击飞行 / 三个 HUD 按钮都已经自动化验过了（`game/regress.py touch`，用 `TouchObject` + `totalTouchPoints` 自己造触点、`dom.w3c_touch_events.enabled=1`）。剩一件事无头验不了：**"是触屏设备才把这三排按钮显出来"的判据在真机上成不成立** —— 无头 Firefox 压根报不出触屏能力（见下面「测试自己的坑」最后一条）。真机反馈还得等用户手机打开 `https://wkgn1.github.io/game/`。
2. 没有音效、没有合成、没有怪物；存档只有"改了什么"，没记录玩家位置朝向？（`save.rs` 里有什么自己看一眼再说）。
3. `game/target/` 300 MB、`dist-web/` 都是中间产物，已在 `game/.gitignore` 里，别提交。
4. wasm 体积还能再压：`Saira.ttf` 476 KB 是站点最大的单个文件（彩蛋本身才 177 KB gzip）。
### 用户手机上打不开（2026-11-18 上午，微信里截图：门面盖着、按钮不见了、写着「这台机器的浏览器跑不了 wasm」）

我看不见他的手机，所以这一轮把「页面自己把原因说出来」做掉了，以后他一截图我就能知道到底哪儿错。

1. **门面里多了一行「原因」**（`#fallback-why`，样式 `.g-why`）：真跑不起来时写具体原因（报错文本 + 哪个文件第几行 + 顺带记录没加载上的资源）。`loader.js` 的 `showFallback()` 和内联脚本的 `mclFatal()` 都会填它；跑成功后 `watchBoot()` 会把它和兜底一起收掉。
2. **「点开玩」按钮不再一藏了之**：以前出错就 `display:none`，用户连重试入口都没有；现在改成「刷新重试」并绑 `location.reload()`。
3. **判死改成延时判定**：出错先记账（`window.__MCL_PENDING`），只有「模块脚本压根没执行」（内核不认 `type="module"`）或者「12 秒还没就绪且期间报过彩蛋自己的错」才亮兜底。原因：wasm 编译在旧手机上要十几秒，2.5 秒就宣判必误杀。
4. **彩蛋不该被站点脚本连累**：内联脚本现在只把 `mq_js_bundle.js`、`loader.js` 和 `blob:`（wasm 运行时）的错算成彩蛋自己的错；`variant.js` 之类配色脚本挂了不算。
5. **macroquad 0.4.16 胶水自带的真 bug**：quad_net 那段写的是 `register_plugin=function(e){...},miniquad_add_plugin({register_plugin,...})`，没 `var`，而整份胶水是 `"use strict"` → 每次加载都往 `window.onerror` 上扔 `ReferenceError: assignment to undeclared variable register_plugin`。挂掉的只有我们没用到的 quad_net，但它会把「跑不起来」的判定骗成假阳性。**`build-wasm.sh` 现在在拷出来的那份上补 `var`**（写法必须是「先单独声明再赋值」：`var register_plugin;register_plugin=...`；直接写 `var register_plugin=...,miniquad_add_plugin(...)` 会让后半句变成 var 列表里的非法项，整份胶水 SyntaxError——踩过一次，靠新的「原因」行当场抓到）。
6. **回归脚本（没进仓库，`/tmp/gdiag4.py`）**：三种情形一起验——① 正常：booted、兜底藏好、原因行为空；② 站点脚本自己出错：彩蛋照样跑起来、不误报；③ 把 `window.WebAssembly` 抹掉：兜底露出、原因行写明「can't access property compileStreaming, WebAssembly is undefined」、按钮变「刷新重试」。三条全过才算好。
7. 结论：**他手机上到底是哪一条，还等他再打开一次的截图**。要看的信息就在门面那行「原因」上。

### 这轮（11-18 晚）：他自己只说了「bug 不少」，清单是我自己找出来的

先说两句要紧的：**「挖了不重画」和「右键放不下去」最后都不是产品 bug，是我那套无头测试自己没准备好**（坑全写在下面「测试自己的坑」）；真正修掉的是这八条。

1. **`watchBoot()` 定义了却没人调用**。`loader.js` 尾部只调了 `registerBridge / loadSave / wireButtons / drainNote`，`watchBoot()` 挂着没人管 → 门面永远不收、HUD 永远带着 `hidden`、八格内容冻在「等待 / 06:00 / 帧 0」、第 8 段（备注）永远没人显示。补一行 `watchBoot();` 之后时刻在走、模式随 F 变、帧率 60、备注弹得出来。我是往页面上挂 MutationObserver 数 HUD 相关的 DOM 改动确认的：修前 30 秒 0 次，修后同样 30 秒 34 次。
2. **「改动」计数不跟着存档走**：`world::from_raw` 把 `changed` 写死成 0 → 刷新之后画面上「改动 0」，可坑明明还在（看着就像"你的改动没存上"）。现在 `from_raw` 拿 `World::new(seed)` 当基准逐格比，不一样就 +1（世界体 128×32×128，比对一次几毫秒）。
3. **在画布外面松手，鼠标键状态会卡住**：胶水的 `mouseup/pointerup` 只绑在画布上，跑着的时候在顶栏、页脚或者画布外头松手，macroquad 那边永远看不到松手 → 之后光是动一下鼠标，视角就自己转。`loader.js` 在 `window` 上捕获相加了 `forwardOutsideUp`：不是画布上收到的松手就原样转交给画布（画布自己收到的不转，免得处理两遍）。实测修前松手后挪 6 下鼠标、画面中心像素和变了 4073（判"画面重画"的阈值是 4000），修后 0。
4. **触屏被同时当成鼠标，点一下挖两格**：`read_touch` 除了触摸还派发模拟鼠标事件，一次轻点先走触摸一遍再走鼠标一遍。现在触摸只走触摸。顺带把说明里写了但代码里没有的**双击开/关飞行**做了，摇杆方向也修了（屏幕向上 = 朝当前朝向前进，和键盘共用那套 yaw 基），按住左半屏走路不再顺手把视角拖走。
5. **手机端没有跳/飞/换方块的入口**（手机上没有键盘，而这三件事原来只有按键）。`index.html` 加了 `#btn-jump / #btn-fly / #btn-slot-prev / #btn-slot-next`，走三个新的 wasm 导入 `mcl_take_jump / mcl_take_fly / mcl_take_slot`（槽位是**带符号的增量**）。要紧的写法：这三个 take 每帧都得各读一次、读完清零，**不能写成 `a || b` 那种短路**，否则前一个没消费后一个就永远读不到。实测点「跳」y 18.00→19.08，点「下一个」槽位 1→2（泥土），点「飞」模式切飞行。
6. **触屏判据太窄**：原来只有 `'ontouchstart' in window || maxTouchPoints > 0`，现在加了 `matchMedia('(any-pointer: coarse)')` —— 带触屏的笔记本、平板、一些国产 WebView 前两个都是空的。
7. **「wasm 起不来」那个模拟原来是假的**（测试侧，但坑很典型）：原来用 `defineProperty` 截 `window.load`，可胶水的 `load` 是**函数声明**、不是赋值，截不住 —— 实测 `__MCL_BOOTED` 照样置位，等于什么都没模拟。改成把 `WebAssembly.compile / compileStreaming / instantiate / instantiateStreaming` 换成必然失败的 promise，这才是"胶水自己 `.catch(e => console.error(e))`、页面上一个字都不说"那一幕。现在 22 秒上门面就写明「原因：wasm 没起来：模拟胶水失败：wasm 起不来」，按钮从灰的「加载中」变成能点的「刷新重试」。
8. **手机上那些细节这轮才真做完**：画布 `position:fixed`（不被 iOS 橡皮筋的 scrollY 带着跳）；关掉 `-webkit-touch-callout` / `user-select` / `tap-highlight`（长按不窜选区）；`env(safe-area-inset-*)` 补刘海和圆角；`maximum-scale=1` 禁双击缩放；`.g-frame` 原来的 `clip-path` **连画布四个角的点击一起剪掉**（改成在背景层画两块纸色三角形，画布本身不吃剪，四个角都留得住）；页脚给最小高度（提示文案一长就别把画布顶得重建 GL）；HUD 数字用 `font-variant-numeric: tabular-nums` 不抖行；门面顶部可达 + `touch-action:pan-y`。

另外这轮还并掉了两处重复：脏表原来有两份（`world.dirty` 和 main.rs 里一份），现在只有 `world.dirty`，由 `rebuild_dirty_chunks` 消费；`mesh.rs` 里 y=0 的底面不再生成（看不见但吃索引）。索引缓冲区显式扩到 `gl_set_drawcall_buffer_capacity(16000, 12000)`：默认 5000 条会被截，macroquad 只是每帧 console.warn 一句 `clamping index write ... exceeded max drawcall index 2`，**表现是远处偶发一条看穿天空的缺面**，特别容易被当成"渲染有 bug"。旧版一轮实测 397 条警告，现在 0。

### 测试自己的坑（下次别再重新踩）

- **macroquad 的输入是跨帧的**：按下要被某一帧看见、松手要被后面某一帧看见。JS 里同一拍里连发 `mousedown + mouseup + click` 什么都不会发生；空格也一样（跳跃读的是 `is_key_down`），但 F 用的是 `is_key_pressed`（边沿检测），同一拍也认。所以 `regress.py` 里点击都带 `hold=0.35`，按键都是 `keydown → sleep 0.7 → keyup`。
- **视角灵敏度是 π/420 每像素**：拖 420 像素就是 180°，早顶到俯仰限位，等于垂直往下看自己脚底那一格。"右键放不下去"十有八九就是这个：那一格正被玩家自己占着，`place()` 里「别把自己砌进墙里」那条必然拒绝。而且垂直往下挖会把**自己脚下的方块挖掉、人掉进坑**，之后往哪放都贴脸。要测放方块就得抬着角度打远处：实测桌面在"比挖的时候抬 360 像素"的俯角上一放就成（改动 3→4）。
- **判"有没有重画"要用中心 20×20 的绝对像素和差**，不能用那个比值：陡俯角挖的坑投不进中心 20×20，比值看着纹丝不动（我被这个骗过一整轮，白写了个"完全不重画"的结论）。空转 2 秒中心像素和漂移实测 0，所以判据写成"变化 > 4000"；一刀实测 +21566。
- **跨会话比"是不是同一个世界"用全画面和的相对差**，容差 0.0040：挖 1 格实测差 0.0539，清档重开实测差 0.0000，两头都对上才算存档这条通了。昼夜一轮只有 360 秒，所以采样必须对齐（都用"开跑后再等 8 秒"，采样之前不许动视角）。
- **`vis()` 得用 `getClientRects()` 并且往祖先看**：`getBoundingClientRect()` 在父层 `display:none` 时照样给矩形，我就被它骗过，把"整排按钮在被藏起来的 `#hud-btns` 里"读成了"按钮出来了"。
- **无头 Firefox 报不出触屏能力**：`'ontouchstart' in window` 是 false、`navigator.maxTouchPoints` 是 0、`(any-pointer: coarse)` 也不成立；往 `window`、`window.wrappedJSObject`、`Window.prototype`、`Navigator.prototype` 上塞都塞不进页面的作用域（四种都试过）。所以"是触屏才显出跳/飞/换方块"这一步在自动化里只能验到：判据代码在（静态检查）、布局是对的（手动把那一排翻开：469×44，四个按钮各 52–66 × 44 px，够手指点）、桥是通的（点下去真的跳/换方块/切飞）。**显不显只能等真机。**
- geckodriver 同一时刻只让开**一个**会话：每个场景收尾必须 `close()`，否则后面每个场景的 `POST /session` 直接 500（我一次跑了 5 个场景全"自己崩了"就是这个）。
- 点「清档重开」是页面自己 `location.replace()`，钩子装得比 glue 的 `getContext` 晚，那一拍 `readPixels` 读回来全是 0。要验清档：先去 IndexedDB 里确认那条记录没了（`db_state()`），再用 `s.go()` 自己导航。
- 存档里「时刻」那格别分两次读：wasm 推的值和页面 DOM 各读一次，中间游戏时钟又走了 1 分钟（1 游戏分 ≈ 0.25 秒真时间），就变成"两边不一致"。一次 execute 里把两个都拿回来就没这事。
- **同一瞬间 DOM 也可能合法地比 `__MCL_HUD` 落后一分钟**（这条和上面那条不是一回事）：wasm 每 0.2 秒推一次，页面每 200ms 才写一次 DOM，两个节拍不同步，而游戏里一分钟只要约 0.25 秒真时间。判据只能写成「在动 + 差距 0~1 分钟」，要求一模一样必红。
- **合成 MouseEvent 也得填 `buttons`**：harness 的 `mouse()` 以前只给 pointer* 事件填 buttons，MouseEvent 没填，而页面 `pushLook` 看的正是 `ev.buttons` —— `undefined !== 0` 直接成立，于是"往右移光标"在自动化里一次都没被当成移动，自由转向全都测不到（现象是"改了但一点效果没有"）。
- **往 JS 字符串里写正则要小心转义**：`\d` 这种经 Python 字符串落到 JS 里会变味（多写一层就成了匹配字面反斜杠），`pose()` 就一直返回 null。判据里的数字直接写 `[0-9.]`，不转义最省事。
- **转向距离会被画布高度卡住**：不锁指针的话，光标从中间到上下边只有约 337 像素（≈86°）。`drag_look` 改成「不按键挪光标」之后，撞到边就用 `hop_cursor` 按住**中键**把光标拉回中线再继续 —— 按着时页面不推视角（不会转），而中键松手在 Rust 那边是「把准星指着那种方块换到手里」，不改地图，所以不污染「改动」计数。拿左键挪就会顺手挖掉一格。
- **pitch 抵着夹紧（±(π/2−0.02)）时再往下拖是量不出变化的**：手机那条一开始读到 pitch −1.551 → −1.551，看着像"往下拖不转"，其实早就到底了。判据必须先往上抬、再往下压。
- **画布外松手那条判据换了口径**：自由转向之下，"按键卡住"的表现不再是"一动鼠标就自转"，而是"之后鼠标再怎么动都不转"（按着的时候本来就不推视角）。所以现在测的是：画布外松手 → 再自由挪 128 像素 → yaw 必须又动起来。

### 这轮（11-19 早）：操作方向三条 —— A/D 反了、鼠标要一动就转、手机拖动要同向

他的原话：「w是前进，s是后退，a是向左，d是向右，在电脑上运行就用鼠标移动控制视角，手机上的视角移动方向和实际拖动方向一致」。三条都是方向问题，实际是两个不同的错：

1. **A/D 走的是屏幕左，不是屏幕右。** `wish_from_keys` 把右手方向写成 `[cos yaw, 0, -sin yaw]` —— 那是「yaw 增大时视线的变化方向」，跟屏幕右是反的。真正的屏幕右是 `cross(视线, 上) = [-cos yaw, 0, sin yaw]`：`Camera3D::matrix()` 走 `Mat4::look_at_rh`（`.cargo/registry/.../macroquad-0.4.16/src/camera.rs:253`），那个矩阵就是把 `cross(f, up)` 当第一列用的，那一列才代表「世界里哪个方向是画面右」。W/S 的符号本来就是对的（所以他只纠正了 a/d，没纠正 w/s）。摇杆那套基（`let right = [cy, -sy];`）是同一个错，一起改。
2. **yaw 符号是反的，鼠标和触摸都反。** 两边都写 `yaw += dx * sens`，而 yaw 增大其实是往**左**转。这条不是推出来的，是量出来的：拿 `Mat4::look_at_rh` 建一个 yaw=+0.3 的相机，把原来正前方十格那个点变换到视图里，落在 x=+2.96（≈ 10·tan 0.3，也就是画面**右边**）—— 画面内容往右滑就说明镜头往左转了。所以「往右拖」必须**减** yaw。俯仰那两条本来就对（往下拖 = 往下看），没动。
3. **修法是把符号收在一处。** `phys.rs` 新增 `look_dir_of / right_dir_of / turn_yaw_pitch`：键盘、摇杆、鼠标拖拽、自由转向全走它们。再加两条单元测试钉着（`屏幕右边的方向由渲染矩阵定而不是靠直觉`、`向右拖必须是往右转而不是往左转`，判据就是上面那个 look_at_rh 变换）。以后谁改反了，测试直接红，不必再重算一遍坐标系。

**桌面「移动鼠标就转向」的实现**：`loader.js` 里的 `pushLook` 盯 `document` 上的 `mousemove`，把位移攒着，wasm 每帧通过三个新导入取走 —— `mcl_free_look / mcl_take_look_x / mcl_take_look_y`（`take_*` 取走即清；`bridge` 里配了非 wasm 的桩，`cargo test` 才链得动）。几个决定要说清：

- **按着鼠标的时候不推视角**（`ev.buttons !== 0` 就跳过并丢掉上一个点）。那一下本来是要挖/放，手难免抖，不然每挖一对方位就跟着抖一下；顺带也免了「按下拖一点再松手」既挖又转。
- **能指针锁定就锁，锁不上也不能坏**。光标在画布里能走的距离有限（画布高约 674 像素，π/700 下从中间到边只有 ±86°），所以点一下画布就 `requestPointerLock()`，锁上后用 `movementX/movementY`，转多少圈都不受限；引擎不认指针锁定（老 Safari、一些微信内核）就退回按 `clientX/clientY` 差分，那种情况下光标离开画布就不再推（他离开画布多半是去点顶上那排存档按钮）。**故意没有做成「只有指针锁定才行」**。
- **自由转向开着时 Rust 不再按拖拽转第二遍**，点击判定也不再拿「拖了多远」当依据（`tapped = free || 距离 <= 6`）：鼠标本来就一直在飘，距离没意义。`free_look` 只在非触屏设备上为真；触屏照旧走右半屏拖动，而且用的是同一个 `turn_yaw_pitch`，所以手机拖动方向和手指一致 —— 这条现在有断言了。
- HUD 第 9 段（界面上不显示、专门给自动化读的）从 `x y z` 扩成 `x y z yaw pitch`，方向类判据才有东西可读。JS 那边只映射前 8 段，多出来的字段不影响界面。

### 现在的实测数字（2026-11-19 早，本机）

- wasm 本体 528,866 字节，gzip 178,040 字节。开跑到 HUD 亮 7–10 秒。HUD 帧率 60–61；30 次采样的浸泡测试后段没有一个低于 30，全程零报错、零截断警告。
- 自动化 **104 条断言：0 失败、3 条不作数**（不作数那三条就是无头报不出触屏能力）。七组：桌面基础 / 静态 / 无 WebGL / 看门狗 / 门面 / 手机触摸 / 浸泡。全屏那轮加了 9 条：静态 4 条（按钮在、走 API + orientation.lock、iOS 退回 CSS 横屏、坐标换算写在原型上），手机触摸里 5 条（画布真变横的 + 换算装上、屏幕往下拖=yaw 减而 pitch 不动、屏幕往右拖=pitch 加而 yaw 不动、退出恢复竖屏、真按钮走「全屏被拒 → 退回横屏」那条分支）。
- Rust 单元测试 28 条全过（`cd game && cargo test`，本地目标就够，不用 wasm）。这轮新加 2 条方向判据（见上一小节：屏幕右由渲染矩阵定、往右拖必须减 yaw）。
- 方向这轮的实测数字，写在这里省得下次再算：桌面光标右移 128 像素 → yaw 0.600 → 0.026，变了 **−0.574**，和 128·(π/700)=0.5749 对得上；再把光标挪回原地，yaw 差 **0.000**。按 S 1.5 秒：沿视线 −6.75、沿屏幕右 −0.00；按 D：沿视线 −0.16、沿屏幕右 **+6.56**；按 A：沿屏幕右 **−5.69**；三次 yaw 变化都是 0.0000（键盘不转向）。手机右半屏往右拖 150 像素 → yaw 变了 **−1.122**（=150·π/420），往上拖 150 → pitch **+1.122**，再往下拖 300 → pitch **−1.122**（抵着夹紧为止）。
- **回归脚本进仓库了：`game/regress.py`**（原来只躺在 `/tmp`，重启就没）。`python3 game/regress.py` 全跑，`python3 game/regress.py base touch` 只跑两组；测的是 `dist/`，所以要先 `bash game/build-wasm.sh && python3 build.py`。它 import 仓库根的 `wdshot.py`，geckodriver 用 `WDDRIVER=http://127.0.0.1:4455` 指过去。老的 `gamecheck.py` 留着（只做"能不能跑能不能挖"，一分钟出头，适合改完快速看一眼）。

### 这轮（11-19 上午）：全屏 / 横屏

他要的是「电脑手机都能全屏，手机上全屏是横屏」。`loader.js` 里 `enterFullscreen()` 按引擎给不给 API 走三条路：

1. **有元素全屏 API 的**（桌面 Chrome/Edge/Firefox、安卓 Chrome、平板 Safari）：整页 `document.documentElement.requestFullscreen()`，进了 `fullscreenchange` 再 `screen.orientation.lock('landscape')` —— 安卓上这一句就横屏了，桌面浏览器不认这句、自己忽略（`catch` 掉，别让它冒错）。退出：再点按钮，或者浏览器自己的 Esc。
2. **没有元素全屏的**（iPhone Safari、微信 iOS 内核 —— 只给视频）：退回 **CSS 横屏**：整页按「视口宽高对调」摆好再转 90°（`width=innerHeight`、`height=innerWidth`、`left=innerWidth`、`transform-origin:0 0` + `rotate(90deg)`；宽高在 JS 里按**像素**写死，vh/vw 在 iOS 上算不准，工具条一收就变）。看着就是横屏，手机横过来拿就行。Esc 或再点一次退出。
3. 两个都不给（个别 WebView）：按下去什么也不变，不装模作样。

**横屏模式的输入坐标必须换算，不换算方向就是错的。** 胶水算输入用的是 `clientX/clientY` 减画布的 `getBoundingClientRect()`（`mq_js_bundle.js` 里的 `mouse_relative_position`），画布被 CSS 转 90° 之后这套坐标对不上。CSS `rotate(90deg)` 把局部 `(x,y)` 画到屏幕上是 `(-y, x)`，反解是 `x = 屏幕y − 中心y`、`y = 中心x − 屏幕x`，再加回画布没转时的左上角（没转时的宽 = 转后的 bounding 高，没转时的高 = 转后的 bounding 宽）—— 就是 `loader.js` 里的 `unrotate()`。

**坑：往事件对象上盖 getter 不顶用。** 第一版是在 window 捕获相给事件对象 `Object.defineProperty(ev,'clientX',{get:…})`。实测：横屏里拖一下**视角一点不动、人倒是往后走了** —— 胶水在我们读到之前就把原始屏幕坐标取走了，而那串坐标对 658 宽的画布落在 **0..329**，也就是左半屏，走的是摇杆那条。改成改 **`MouseEvent.prototype` / `Touch.prototype` 上的 getter**（`fixInputCoords()`：进横屏时装上、退出时把原描述符装回去），就跟监听注册的先后无关了，谁读都是换算过的；`PointerEvent.prototype` 继承 `MouseEvent.prototype`、自己没有 `clientX`，所以改 `MouseEvent` 就够。装没装上留了个 `window.__MCL_ROT_PATCH` 给测试和排查看。
判据是硬的：换算对了，屏幕上「往下拖」= 画面里往右（yaw 该减）、屏幕上「往右拖」= 画面里往上抬（pitch 该加）；换算反了这两条正好反过来。

自动化留了两个便利钩子：`window.__MCL_SET_ROTATE(1/0)` 直接开关横屏（无头浏览器没有用户手势走不到真全屏，这个钩子保证坐标换算那条路每轮都被测到）；无头里点真按钮会被 `requestFullscreen` 的用户手势要求拒掉，正好把「全屏失败 → 自动退回横屏」这条分支也验上了。

**`check-es.py` 的假失败**：它用 npx 拉 acorn，网络不通时报「acorn 超时」并把那几处**判成超标**（实测一次 FAIL 1/5、紧接着重跑 PASS）。真要判 ES5 看 `game/regress.py` 静态场景里那条不依赖网络的：去掉注释和字符串之后找 `=>`、`const`、`let`、反引号、`??`、`?.`、`class`。

### 这轮（11-19 中午）：背包 / 快捷栏 / 工作台，分创造和生存

他要的是「物品按钮改为物品栏，增加背包和工作台系统」，追问之后定成 **1A + 2A + 分创造和生存**：挖了掉进背包、放下扣一个（生存）；合成出工作台方块、放地上站旁边才解锁高阶配方；并且要有创造 / 生存两个模式。默认开**创造**（和改之前手感一样：随便挖随便放）。

**物品就是方块 id**（`inv.rs`）：不开第二张物品表，存档、图集、HUD 全共用同一套编号，少一层对不上的可能。36 格 = 快捷栏 9（对数字键 1..9 和滚轮）+ 背包 27（只有面板里点得到）；一格最多叠 64（`STACK_MAX`）。创造模式**数量永远记 0**、界面画 ∞，挖不掉料、放也不扣。

**创造↔生存切换会把背包换成那个模式该有的一套**（`toggle_creative`）：创造记 0、生存看数量，混着带过去就是「手里有 0 个的方块」，看着跟 bug 一样，所以切了就重给。生存开局：草 8 / 土 8 / 石 8 / 木 4（够搭个窝 + 合成第一张工作台）。切换入口：电脑上按 `G`，面板里那个「改成生存 / 改成创造」按钮（命令 300）。

**配方 5 条**（`inv.rs::RECIPES`，每条带一句给玩家看的 `note`，HUD 第 8 段就吃这个静态串）：
- 木头 1 → 木板 4（不要工作台）
- 木板 4 → 工作台 1（**故意不要工作台**：不然第一张永远做不出来）
- 树叶 4 → 木头 1、石头 3 + 木头 1 → 萤石灯 1、雪块 2 + 树叶 1 → 雪草 1（要工作台）
`craftable()` 三条都查：料够、**产物还塞得下**（`room_of`，不然料扣了产物没地儿去）、要工作台的得 `bench_near()`（水平 4 格、上下 2 格内有 `WORKBENCH`）。创造模式不看数量，所以那边合不成，面板上直接写着「创造模式不看数量，合不成」。

**数据怎么到页面**：走 HUD 管子的**第 10 段**（`inv::inv_text`），所以 HUD 从 9 段变 10 段（`regress.py` 里那条「HUD 有 9 段」跟着改成 10）。段内**一个 `|` 都不能有**（管子按 `|` 切），字段用 `;` 分、项用 `,`、键值用 `:`、名字表用 `id-名字`、配方用 `产物:数量>料:料` 加 `*` 表示要工作台：

```
C:1;B:0;S:1;N:1-草方块,2-泥土,…,12-工作台;H:1:8,2:8,…（9 项）;G:0:0,…（27 项）;R:7:4>6:1,12:1>7:4,…;A:1,0,0,0,0
```

页面 `parseInv()` 按**字段名**取（不是数第几段），往后加字段不用改解析。名字表也在这一段里：中文方块名只有一份，不会 JS 和 Rust 各维护一份然后对不上。

**操作怎么回 Rust**：`env.mcl_take_ui_cmd()`（照 `mcl_take_slot` 的形状：页面塞队列、Rust 每帧取一条，取走即清；非 wasm 恒 0，`cargo test` 才链得上）。命令号：

| 命令 | 意思 |
| --- | --- |
| 1..9 | 选快捷栏第 n 格 |
| 100..126 | 背包第 n 格 ↔ 手上那格对调 |
| 200..299 | 合第 k 条配方 |
| 300 | 切创造 / 生存 |
| 400 / 401 | 面板开（= 暂停）/ 关 |
| 6000 + id*100 + 数量 | 给料，**自动化和调试用的**；创造不看数量所以忽略 |

面板开着 = `game.paused`：`read_mouse` 整条不走、摇杆归零、跳和 F 都不响（电脑上怕你边看边跑，手机上面板本来就挡着屏）。挖/放/走都不生效，但 1-9、合成、切模式照旧（都是命令队列来的）。

**存档升到 v2**：头部 21 字节（v1 是 17，多的 4 字节是 `inv_len`），尾巴定长 110 字节 = 模式(1) + 选中格(1) + 每格 (id, 数量小端 u16)。**`MAGIC` 还是 `MCL1`** —— 页面导入时那一眼看的是前四个字节，改魔数会把导出/导入判成坏档。`decode` 按版本号分派：v1 走 17 字节头、背包给 `None`（页面就用创造那套），v2 才读背包段；`inv_len` 先夹到 4096 再算 `total`（`checked_add` 两级），不然切片直接越界。读回来的背包段是**要么全收要么不收**：长度、方块 id 上限、`STACK_MAX`、"空格里挂着数量"、"生存里 0 个的格子" 任何一条不对就整段丢掉，宁可丢背包也不能让一个本来能读的存档变成读不回来。

**两个自动化上的坑（这轮真撞到的）**
- **`TOOL.mouse()` 的 mouseup 必须把 `buttons` 清成 0。** 之前它 `buttons: btn || 0` 通用于按下和抬手，于是右键抬手在胶水那边看成长着右键没松过，`is_mouse_button_released(Right)` 永远假 —— 右键放方块在页面上"点了没反应"。浏览器本来就在 mouseup 把 `buttons` 清 0，合成事件得照做。老的挖方块判据都用左键（0 → `buttons: 0` 正好对），所以这条一直没人发现；判据只测左键就会漏掉整条右键路径。
- **只跑 `python3 build.py` 不会更新 `static/game/`。** `game/web/loader.js` 改完之后必须 `bash game/build-wasm.sh`（它负责把 `game/web/*` 拷进 `static/game/`、补胶水、算 wasm 哈希写 manifest），只跑 `build.py` 测的是**旧 loader**：那次调了半天"背包那 27 格画不出来"，其实页面根本没拿到新代码。判据跑之前先确认 manifest 里的哈希变了。
- 顺带：按下和抬手必须被不同帧看见（老坑，新判据又踩一次），所以挖/放一律 `press` → sleep → `release`，别用 `clickAt` 一把梭。

**没做的（故意的，别当 bug 找）**：没有血量/死亡，掉出世界就是掉出去；背包不区分工具，什么方块都能徒手挖；配方只有 5 条，没有熔炉/工具/火把；工作台没有单独碰撞和动画；创造不能合成（那边本来就全套都有）；存档还是不存玩家坐标（和以前一样）。

**这轮的数**：Rust 单测 28 → **40 条**（`inv.rs` 8 条：扣料、塞不下就不扣、工作台门槛、`inv_text` 里没有 `|` 而且字段齐 …；`blocks.rs` 2 条：每种方块每一面都用得上贴片、工作台是实体方块也在创造全套里；`save.rs` 2 条：v1 老档还读得懂、背包段长度被吹大必须报错不能越界）。自动化判据 104 → **125 条**（base 场景里 17 条背包/工作台：第 10 段字段齐、面板 36 格 + 5 条配方、开着面板挖不动、切生存、挖了 +1、放了 −1、合成扣料出产物、木板→工作台、点格子换到手、放地上 B 段变 1、高阶配方解锁且真能合、切回创造不扣料、给玩家留话；静态 4 条：第 10 段有人接、物品栏入口在而旧按钮没了、面板壳和样式齐、按字段名读）。全 7 组 **125 条 / 0 失败 / 3 条不作数**。wasm 528,866 → **553,313 字节**（gzip 178,040 → **186,054**）。

**判据脚本自己也修了两个真 bug**（不是产品的，但下次再写判据得避开）：
- 触屏场景还点着已经删掉的 `btn-slot-next`，`getElementById` 返回 null 调 `.click()` 直接把整组判据崩掉 —— **删产品上的元素要连着搜一遍判据脚本**，光看「静态检查里有提到」不够。
- 静态判据里我写 `"'R'" in L`（带单引号）而 loader 其实是 `f.R` 这么读，判据永远 ✗。**判据里引用的字面串要从被测文件里 grep 出来抄**，别照着记忆写。

### 学一下：OpusMC（他丢来 BV1HkJ66vEk9 问「效果最好的代码」）

**是哪个**：那期是 4 个模型各造一个 MC（GPT-6 Astra / 豆包 Pro 2.1 / Claude Opus 5.5 / Kimi K3），效果最好的是 **Claude Opus 5.5 那版**——UP 主自己置顶的游玩地址就叫 **OpusMC**，后来又单独给它出了一期「耗资上万！我重制了网页版MC」（BV1Nxxr6oES2，183 万播放，全程 Claude Opus 5.5，还加了匠魂/农夫乐事模组）。官方地址 `bilibili.com/toy/OpusMC` **已下架**，UP 主的说明在 BV1rxsQ6CEus：「玩法和材质来自 Minecraft，确实侵权」。

**代码从哪儿读到的**：官方入口撤了但镜像还在。拿到一份**原版单文件镜像**（`res.sparklight.cn/…/OpusMC-原版注入版__5cc391aa1cf4c0cf.html`，签名地址有效期到 2036 年），**50,140,022 字节**、1,021,824 行、114 个内联 `<script>` 块，**没压缩过，能直接读**。

**体量对比**（这条得记住，别被"效果差距"唬住）：它光一个叫「WebGL2 renderer」的 script 块就 **133,393 字符 / 3,183 行 / 189 个顶层定义**；贴图是把 **1587 张 PNG 以 base64 塞进 HTML**（合计 13,353,212 字符）；两个模组是另外的 bundle.js；地形用的是 MC 那套噪声配置数据（aquifer / find_top_surface 等，也是塞在 HTML 里的 JSON）。我们是 **3,933 行 Rust + 2,479 行前端**，wasm 553,313 字节（gzip 186,054）。差两个数量级，**但它手机优先的思路跟我们是一条路上的**，所以能抄的东西不少。

**它凭什么好看**（按"我们能抄的程度"排）：

| 它的做法 | 我们的现状 |
| --- | --- |
| 顶点 AO + 面朝向明暗 + 对角翻三角形（AO 最暗的那对角翻转，免得出现明暗斜切不一致） | **已经有了**（`mesh.rs::ao_level`、`FACE_SHADE`、`AO_SHADE`、`ao0 >= ao1` 翻三角形） |
| 距离雾：`mix(color, fogColor, clamp((d-near)/(far-near)))`，而且 `d = max(水平距离, 竖直距离*0.5)`（竖直方向压一档，免得低头看脚就起雾）；**视距就等于雾距**，chunk 冒头被雾盖掉 | **已经有了**（`u_fog_near/far`），但"竖直距离打五折"和"视距=雾距"这两点我没做 |
| 昼夜压暗 + 夜色染色 + 环境光：`u_day` / `u_nightTint` / `u_ambient`，再加一条**亮度（gamma）曲线** `mix(x/(4-3x), 1-(1-x)^3, u_gamma)` 给玩家当亮度旋钮 | 昼夜压暗有（`u_day`），**gamma/亮度旋钮没有** |
| 水面/岩浆动画：顶点里存"这种方块有几帧动画"，片元里 `layer += floor(u_tick/2.0) % anim` —— **不动网格、不重建，零成本换贴图帧**；紧凑区块段还把 anim 塞进 `a_pos.w` 省带宽 | **没有**：水是静态贴片，换帧得重建网格 |
| 光照是**查表**：顶点存 (天光, 方块光) 两个数，片元拿它们去采样一张 16×16 的 `u_lightmap` 调色板纹理 → 光照颜色/色温在 CPU 侧算好，着色器里一次纹理采样搞定 | 没有光照系统（只有朝向明暗 + 自发光），真要加光，这张调色板是最省事的形状 |
| **显存自己管**：mesh 顶点 buffer 分到几个大 page 里，page 内 first-fit + 空闲合并 + 整空才回收，VAO 也跟着搬（arena compaction） | 抄不动（macroquad 不给 VAO/buffer 控制） |
| **GC 抠着省**：注释里写「一帧 419 个 typed array、28 只怪在视野里时 39 只，手机会被 GC 停住」，于是把数组池化复用 | 我们 JS 侧很薄（HUD 写文字都做了"变了才写"），暂时没这个问题 |
| uniform **变了才 set** | 同思路（我们 HUD 那边一样的写法），着色器 uniform 也是每帧照写，量小无所谓 |
| 模组系统：模组的贴图/代码 base64 塞进 HTML，运行时 `URL.createObjectURL(new Blob(...))` + `importScripts` 拼出**生成 Worker** 的源码 | 抄不动也不需要 |

**三条它踩过的手机坑（注释里都写着，值得抄进我们的经验库）**：
1. **半精度**：纹理层号本来是 float varying，在 iPhone/半精度手机上 `layer` 超过 2048 就对撞了（两个不同层被当成同一个），最后改成整数比较。我们的片元是 `precision mediump float` + `varying lowp vec2 uv`，**lowp 的 uv 在 224 像素宽的图集上误差能到将近 1 像素** —— 同类坑，值得把 uv 提到 mediump/highp（`shade`/`emissive` 留在 lowp 没问题，它们是 0..1 的乘数）。
2. **iPhone/iPad 上图标串位**：「一根棍子显示成饼干」这类，根子在采样/缓存选择上；他们的解法写在小注释里（低层级最近邻采样、绑定时不许落到 1/2/8 这几个单元）。
3. **B 站 Toy 平台会往游戏里注入一段采集脚本，每 300 ms 读一次画面**，把主线程拖死（玩家卡在 98%）。镜像作者专门做了套壳把它屏蔽掉才跑得动 —— **页面里挂别人的脚本是要还性能的**，我们这页现在只有自家 JS + wasm，别乱加第三方统计。

**给我们的启发（诚实那条）**：它最好看的一份，代价是把 MC 的材质和玩法直接搬进来 —— 为这个下架的。我们那些贴片全是 `blocks.rs` 里程序画的（`TILE_*` 那几个 painter），一个像素都没从 MC 搬，所以**该学的是它的算法和手机经验，不是它的贴图**。「看着像 MC」这件事我们靠体素本身就够了。

**下一轮想抄的（按性价比）**：① 把 `uv` 的精度提上去 + 雾的竖直距离打五折；② 抄那条 gamma 曲线，加一个「亮度」开关（手机上真的有用）；③ 水面动画按它的"多存一帧/换层"思路做（我们可以在图集里多画 2-3 帧水，每 tick 换一次，不动网格）；④ `webglcontextlost` 兜底（现在 `grep -c webglcontextlost game/web/loader.js` = 0，手机切后台/驱动抽风回来就是黑屏，我们有 watchdog 但没接这个事件）。

### 这轮（11-20 凌晨）：把从 OpusMC 学来的东西落成四条

四条全做了（上一轮跟他报的那份「按性价比排的 ①②③④」）：

1. **贴图采样精度**：片元里 `varying lowp vec2 uv` → `mediump`。lowp 只有 8 位有效位，在 224 像素宽的图集上误差快够到 1 像素，采过界就是「一根棍子画成饼干」那类串贴片 —— OpusMC 的注释里为半精度栽过两个跟头（纹理层号 float 超过 2048 对撞、iPhone 上物品图标串位）。`shade` / `emissive` 是 0..1 的乘数，留 lowp 没意见，只把 uv 提上去。
2. **雾的距离改成 `max(水平距离, 竖直距离 × 0.5)`**（`main.rs` 的 FRAG）：以前是 `distance(wpos, u_eye)` 三维直线距离，低头看脚底三五个方块也被算进雾里。竖直方向打五折是 OpusMC 那条。
3. **亮度三档**：FRAG 里加一条抬中间调的曲线 `1-(1-x)^3`，**跟「原样的自己」插值**（`mix(col, lift, u_bright)`）—— 这样 0 档就还是今天这个样子，不会因为我加了曲线先把整体压暗一截（OpusMC 是在一条保守曲线和这条之间插，默认值就不是原样）。高光（自发光那种 >1）留在曲线外面不被掐掉。档位 `0 / 1 / 2` → `BRIGHT = [0.0, 0.45, 0.78]`，页面顶栏那个「亮度」按钮切（电脑按 `B`），命令号 **410+n**。**存在 localStorage 不进存档**：这是显示偏好，换设备该跟着浏览器走、不该跟着世界走，所以没动存档版本（还是 v2、MAGIC 还是 `MCL1`）。开局页面把存下来的档位发一条 410+n 给 wasm。
4. **水面换帧**：`blocks.rs` 里水拆成三帧（`WATER_FRAMES = 3`，只有波纹相位随帧走，**噪点抖动不动** —— 连噪点一起换就是一锅乱闪，不像水在动），图集里水**还是只占一格**（`TILE_WATER`），运行时每 0.5 秒把第 k 帧用 `Texture::update_part` 覆写那一格（16×16，1024 字节）。不重建 chunk、不给顶点加"第几帧"这一维。OpusMC 那边是靠纹理数组的层号 `layer += floor(u_tick/2) % anim` 换帧，我们没有纹理数组，覆写同一格是等价的省事做法。暂停时不换 = 水也冻住，跟「暂停就是停住」一致。
5. 附送一条：**`webglcontextlost` 兜底**。手机切后台、系统回收显卡、驱动抽风都会弄丢 WebGL 上下文，macroquad 不自己恢复，画布就永远停在最后一帧或变黑，看着像游戏卡死。现在接住事件 + `preventDefault` + 复用 `showFallback` 那句话（「显卡的 WebGL 上下文断了……存档每 3 秒存一次，刷新回来最多丢 3 秒」）+ 一键刷新。之前 `grep -c webglcontextlost loader.js` 是 0。

**数字**：Rust 单元测试 **41 条**（+1：水三帧彼此不同、平均亮度接近、图集里那一格等于第 0 帧）；自动化 **136 条断言：0 失败、3 条不作数**（不作数那三条还是无头 Firefox 报不出触屏能力）。wasm **553,313 → 555,986 字节**（gzip 186,054 → **187,444**）。新增 11 条判据：桌面场景 5 条（第 9 段尾巴带 `wf`/`br`、水会换帧、亮度切到最高画面像素和真的变、按钮文案跟着变、刷新后档位还在）+ 静态 6 条（uv 不再是 lowp、雾的竖直打折、亮度是跟原样插值、顶栏有按钮且存 localStorage、接住了 contextlost、水三帧只覆写一格）。

**HUD 第 9 段尾巴多了两个自动化字段** `wf<n> br<n>`（当前水帧 / 亮度档位）。加在段尾是安全的：`__T.pose()` 那种正则解析只取它要的五个数，尾巴多出来的不碰；这样水和亮度都能在画面外判，不用靠我截图猜。命令表也扩了一档：**410..412 = 亮度**（`loader.js` 里 `uiCmd()` 那条注释还是活表）。

**这轮踩的坑（下次自己先看这些）**：
- **别拿墙上时间卡游戏里的量**：水的判据第一版睡 1.4 秒判「wf 该变了」，结果在无头软件渲染下失败了 —— `dt` 被钳在 0.1，帧率一低游戏里的钟就比墙上的慢，水是真的在慢放，不是坏了。改成「最多等 8 秒，一跳就算过」并且把等了多久写进判据。同理以后凡按"游戏里的时间"判的都得这么写。
- **转头靠的是鼠标位移**：`__T.moveTo(x, y)` 挪到**同一个点** = 位移 0 = 不转。第一版拍照脚本每次都 moveTo 到同一个 y，pitch 死活停在 -0.15。要一格一格挪。
- **拍照脚本要点"玩家看到的东西"**：第一版我去正则解析 HUD 第 10 段找水，解析错了（那段的字段是 `;` 分隔、格子是 `id:n`，我按 `|` 和字母找），结果 `-1`。改成在 DOM 里找写着「水」的那格 `[data-cmd]` 点它 —— 跟玩家用手指点的同一条路，也更容易看出是不是产品的问题。
- 其它小的：`Sess` 的收尾是 `close()` 不是 `quit()`；PIL 的自动拉伸对比在 `ImageOps.autocontrast` 不在 `ImageChops`；`pkill -f "timeout 2400 python3 …"` 又把我自己的 bash 干掉了（同一串命令里既有 pkill 又有那条命令的文本），改用 `kill $(pgrep -x geckodriver)`；无头 Firefox 的窗口**最小宽度约 500 像素**，所以"手机视口"在这儿其实是最小 500 宽，不是 390，别把这句话当成 390 的结论。
- macroquad 0.4.16 的 `Image` **没有 `from_rgba`**（字段全 public，直接拼字面量），`Texture2D` **不是 Copy**（`if let Some(ref atlas)`），子矩形上传是 `update_part(&Image, x, y, w, h)`。图集是 `FilterMode::Nearest` + UV 内缩半像素，所以覆写那一格不会漏到邻贴；mipmap 层不会跟着刷新，远处水面看到的是第 0 帧的 mip，近处才是动画 —— 接受。

**还没做的（下次想接着抄的话，按性价比）**：光照调色板那张 16×16（它那边顶点只存 (天光,方块光) 两个数，颜色查表，加光照最省的形状）、火把那种会抖的方块光、视距=雾距（我们现在世界是一整块岛，冒头问题不明显）、显存 page/arena 和 VAO 那套（macroquad 不给控制，抄不动）、模组系统（不需要）。
