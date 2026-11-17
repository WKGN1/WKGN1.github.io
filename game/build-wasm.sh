#!/usr/bin/env bash
# 一条命令产出 static/game/ 里的全部产物。
#
# 为什么只有一条 cargo 命令：macroquad 在浏览器里不用 wasm-bindgen，
# 它用自己的 JS 胶水（mq_js_bundle.js）+ 裸 wasm 导入。少一层工具链，
# 就少一类「本地能跑、CI 跑不出来」的问题。
#
# 用法：bash build-wasm.sh
set -euo pipefail
cd "$(dirname "$0")"

# 找 cargo：先用 PATH，再用仓库外的 workspace 安装（本机 rustup 装在 ../.cargo）
CARGO_BIN="${CARGO:-}"
if [ -z "$CARGO_BIN" ]; then
  if command -v cargo >/dev/null 2>&1; then
    CARGO_BIN="$(command -v cargo)"
  else
    p="$PWD"
    for _ in 1 2 3 4 5; do
      if [ -x "$p/.cargo/bin/cargo" ]; then CARGO_BIN="$p/.cargo/bin/cargo"; break; fi
      p="$(dirname "$p")"
    done
  fi
fi
if [ -z "$CARGO_BIN" ]; then
  echo "找不到 cargo。先装 rustup 和 wasm 目标：" >&2
  echo "  rustup target add wasm32-unknown-unknown" >&2
  exit 1
fi

# workspace 内安装的工具链需要这两个变量指向仓库外那份
if [ -n "${CARGO_HOME:-}" ] && [ -d "${CARGO_HOME:-}" ]; then :; else
  case "$CARGO_BIN" in
    */.cargo/bin/cargo) export CARGO_HOME="$(dirname "$(dirname "$CARGO_BIN")")" ;;
  esac
fi
if [ -n "${RUSTUP_HOME:-}" ]; then :; else
  if [ -n "${CARGO_HOME:-}" ] && [ -d "$(dirname "$CARGO_HOME")/.rustup" ]; then
    export RUSTUP_HOME="$(dirname "$CARGO_HOME")/.rustup"
  fi
fi

echo "==> 用 $CARGO_BIN 构建 wasm（release）"
"$CARGO_BIN" build --release --target wasm32-unknown-unknown

# crate 名带连字符，cargo 产出的文件名也是连字符
WASM="target/wasm32-unknown-unknown/release/mc-lite.wasm"
[ -f "$WASM" ] || { echo "没产出 $WASM" >&2; exit 1; }

OUT="dist-web"
rm -rf "$OUT"
mkdir -p "$OUT"

cp web/index.html web/game.css web/loader.js "$OUT"/
rm -f web/save_glue.js 2>/dev/null || true

# 胶水脚本必须和 macroquad 版本配套：优先用 cargo 缓存里那份（版本一定对得上），
# 找不到才退回仓库里提交的副本，并提醒可能不匹配。
MQ_SRC=""
for base in "${CARGO_HOME:-$HOME/.cargo}/registry/src" "$HOME/.cargo/registry/src"; do
  if [ -d "$base" ]; then
    MQ_SRC="$(find "$base" -maxdepth 3 -type d -name 'macroquad-0.4.*' 2>/dev/null | sort | tail -1)"
    [ -n "$MQ_SRC" ] && [ -f "$MQ_SRC/js/mq_js_bundle.js" ] && break
    MQ_SRC=""
  fi
done
if [ -n "$MQ_SRC" ]; then
  echo "==> 胶水取自 $MQ_SRC"
  cp "$MQ_SRC/js/mq_js_bundle.js" "$OUT/mq_js_bundle.js"
  echo "$MQ_SRC" | sed 's/.*macroquad-//' > "$OUT/.macroquad-version"
elif [ -f web/mq_js_bundle.js ]; then
  echo "==> 找不到 cargo 缓存里的胶水，改用仓库副本（版本对不上会报 Version mismatch）" >&2
  cp web/mq_js_bundle.js "$OUT/mq_js_bundle.js"
else
  echo "致命：没有 mq_js_bundle.js，wasm 起不来" >&2
  exit 1
fi

# wasm 文件名带内容哈希：站点上就不会被 CDN 缓存成旧版
HASH="$(sha256sum "$WASM" | cut -c1-10)"
NAME="mc-lite.$HASH.wasm"
cp "$WASM" "$OUT/$NAME"
find "$OUT" -maxdepth 1 -name 'mc-lite*.wasm' ! -name "$NAME" -delete

SIZE="$(stat -c%s "$OUT/$NAME")"
GZIP_SIZE="$(gzip -9 -c "$OUT/$NAME" | wc -c)"
BROT_SIZE=""
if command -v brotli >/dev/null 2>&1; then
  BROT_SIZE="$(brotli -q 11 -c "$OUT/$NAME" | wc -c)"
fi

python3 - "$OUT/$NAME" "$SIZE" "$GZIP_SIZE" "$BROT_SIZE" <<'PY'
import json, pathlib, sys
name, size, gzip_size, brot = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
data = {
    "wasm": pathlib.Path(name).name,
    "bytes": size,
    "gzip": gzip_size,
    "note": "wasm 本体体积；站点上浏览器还会再压一次，实际传输看 gzip/brotli",
}
if brot:
    data["brotli"] = int(brot)
pathlib.Path(sys.argv[1]).parent.joinpath("manifest.json").write_text(
    json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
)
PY

SITE="../static/game"
mkdir -p "$SITE"
find "$SITE" -maxdepth 1 -type f -delete
cp "$OUT"/* "$SITE"/ 2>/dev/null || true
find "$SITE" -maxdepth 1 -name '.macroquad-version' -delete

echo
echo "==> 产物已放进 $SITE："
ls -la "$SITE" | sed 's/^/    /'
echo
echo "wasm 本体 $(printf "%'d" "$SIZE") 字节，gzip 后 $(printf "%'d" "$GZIP_SIZE") 字节${BROT_SIZE:+，brotli 后 $(printf "%'d" "$BROT_SIZE") 字节}"
echo "本地起个服务试试：  python3 -m http.server -d $SITE 8000  然后开 http://127.0.0.1:8000/index.html"
