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
