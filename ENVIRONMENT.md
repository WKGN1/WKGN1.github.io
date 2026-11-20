# 本机配置清单

2026-11-20 上午抓的。里面只有版本号、路径和命令，**没有任何密钥内容**（推送走 SSH key，密钥本身不在这儿）。
`/tmp` 底下那些脚本会被清 —— 真要靠它们干活，看下面「自动化」那节，仓库里有底。

## 这台机器

| | |
| --- | --- |
| 系统 | Ubuntu 26.04.1 LTS（内核 7.0.0-43-generic） |
| CPU | Intel Xeon E5-2698 v3 @ 2.30GHz，逻辑核心 32 |
| 内存 | 62 GB |
| 仓库所在盘 | 还剩 667G，挂载点 `/run/media/suyzz/e2270998-5ad9-4aef-860c-ba06f27ea0a9` |
| `/home` | 还剩 402G |
| 中文字体 | 89 个（拼图时用的是 `/usr/share/fonts/opentype/noto/` 底下的 Noto CJK） |

## 版本

| 工具 | 版本 | 在这项目里干嘛 |
| --- | --- | --- |
| Python | 3.14.4 | 建站（`build.py`，只用标准库）、跑自动化判据 |
| Pillow | 12.1.1 | 截图拼接、加中文标题条、放大两帧的差 |
| rustc / cargo | 1.100.0（2026-11-09） | 游戏本体 |
| 已装 target | `wasm32-unknown-unknown`、`x86_64-unknown-linux-gnu` | 前者出浏览器能跑的 wasm，后者跑单元测试 |
| macroquad | `0.4`（实装 0.4.16） | 游戏的渲染 / 输入层 |
| Firefox | 149.0.2 | 无头浏览器，自动化判据跑在它上面 |
| geckodriver | 0.36.0 | 驱动 Firefox |
| curl | 8.18.0 | 查线上文件是不是新的、抓 B 站的接口 |

## 仓库和站点

- 仓库：`WKGN1/WKGN1.github.io`，分支 `main`，推送地址是 **SSH**（`git@github.com:WKGN1/WKGN1.github.io.git`）。
- git 全局**没配** `http.proxy`、**没配** credential helper，`~/.git-credentials` 不存在 —— 所以推送靠机器上的 SSH key，别以为有个 token 在等着。
- 本地位置：`…/dsh_wechat/blog`。33 个提交，`.git` 5.0M，工作区 386M（大头是 wasm 的历史版本和 dist）。
- 站点：<https://wkgn1.github.io>（GitHub Pages，公开）。游戏在 <https://wkgn1.github.io/game/>。
- 推送上线的实测时间：`git push` 之后 **40-60 秒** 线上文件就换了（我每轮都是推完等一下直接 curl 线上文件核对）。

## 命令（可以直接抄）

编译前**每条 bash 都要先导出**这套环境变量（cargo/rustup 不在默认位置）：

```bash
export CARGO_HOME=/run/media/suyzz/e2270998-5ad9-4aef-860c-ba06f27ea0a9/strata/dsh/dsh_wechat/.cargo
export RUSTUP_HOME=/run/media/suyzz/e2270998-5ad9-4aef-860c-ba06f27ea0a9/strata/dsh/dsh_wechat/.rustup
export PATH="$CARGO_HOME/bin:$PATH"
```

```bash
cd …/dsh_wechat/blog
cargo test --manifest-path game/Cargo.toml   # 单元测试（现在 41 条）
bash game/build-wasm.sh && python3 build.py  # 这两条必须都跑，顺序别换
python3 check-es.py                          # 前端语法门禁（页面上只许 ES5）
python3 game/regress.py                      # 全套自动化：150 条断言，约 25 分钟
```

**`game/build-wasm.sh && build.py` 必须都跑**：`game/web/` 底下的东西是 `build-wasm.sh` 搬进 `static/game/` 的，只跑 `build.py` 会拿着旧页面测新代码（我栽过两次）。

## 自动化在哪

- 判据脚本：**`game/regress.py`**（仓库里，1,594 行，才是底）；`/tmp/gqa.py` 是同一份的工作副本，改完要 `cp` 回来并 `cmp` 一下。
- 跑法：`WDDRIVER=http://127.0.0.1:45NN timeout 2400 python3 -u game/regress.py`。**`WDDRIVER` 必须在启动前设好**（脚本 import 时就读它），漏了就默认 4444 → 连不上。端口每轮换一个新的，别复用。
- 拍照 / 线上核对脚本：`/tmp/gshot*.py`、`/tmp/glive*.py`（**都在 /tmp，机器重启就没了**，需要就得重写）。
- 截图输出目录：`/home/suyzz/blogshot/`（现在 71 张）。
- 当前状态：150 条断言 / 0 失败 / 3 条不作数（不作数那三条是无头 Firefox 报不出触屏能力）。

## 别踩这几条

1. `pkill -f "…python3 …"` 会把**我自己那条 bash** 一起干掉（同一串里既有 pkill 又有它匹配的文本）。清残留用 `kill $(pgrep -x geckodriver)`。
2. 无头 Firefox 的窗口**最小宽度约 500 像素**，"手机视口"在这儿其实是最小 500 宽，不是 390。
3. 软件渲染时 `dt` 被钳在 0.1，**游戏里的钟比墙上慢** —— 按"游戏里的时间"判的断言不能硬卡墙上秒数。
4. 微信这个渠道 `send_to_wechat` 发**非图片文件会被拒**（`.js`、`.txt` 都是 `prepare failed`）；要给他文件就走站上链接。
5. B 站的官方玩具页会注入一段采集脚本每 300 ms 读画面（看别人作品时卡 98% 就是这个）。

## 手头其它东西

- OpusMC（B 站那个最好看的网页版 MC）：全文存档 `/home/suyzz/opusmc/opusmc-全文.html`（50,140,022 字节）+ 三个抠出来的小块；站上链接 `wkgn1.github.io/files/opusmc-shaders.txt`、`…-renderer.txt`、`…-world-light.txt`。
- 这轮之后的进度、规则、踩过的坑：全写在 **`HANDOFF.md`**，那才是交接主文档，这份只是"机器长什么样"。
- 定时提醒：当前**没有**挂着的一说一（schedule 列表是空的）。
