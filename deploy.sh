#!/usr/bin/env bash
# 一条命令把站点推到 GitHub 并打开 Pages（自动构建发布）。
# 用法：
#   GH_TOKEN=github_pat_xxx  bash deploy.sh 你的用户名 仓库名
# 前提：
#   - 仓库已经建好（Public、空的最好；有 README 也会被覆盖）
#   - token 要有这个仓库的 Contents:RW 和 Pages:RW
# 之后每次改完内容，只要：git add -A && git commit -m "..." && git push 就会自动重新发布。
set -euo pipefail

OWNER="${1:?要 GitHub 用户名}"
REPO="${2:?要仓库名}"
TOKEN="${GH_TOKEN:?先设置 GH_TOKEN=你的token}"
cd "$(dirname "$0")"

python3 build.py
git add -A
git -c commit.gpgsign=false commit -m "更新站点" >/dev/null 2>&1 || echo "没有新改动，照样推送"

git remote remove origin 2>/dev/null || true
git remote add origin "https://x-access-token:${TOKEN}@github.com/${OWNER}/${REPO}.git"
git push -u origin main --force

api() {
  curl -s -o /tmp/pages.json -w '%{http_code}' \
    -H "Authorization: Bearer ${TOKEN}" -H "Accept: application/vnd.github+json" \
    -H "Content-Type: application/json" "$@"
}
CODE=$(api -X POST "https://api.github.com/repos/${OWNER}/${REPO}/pages" -d '{"build_type":"workflow"}') || true
case "$CODE" in
  201|20*|30*) echo "Pages 已开启（$CODE）" ;;
  *) echo "Pages 可能已开过（$CODE），改用更新方式"
     CODE=$(api -X PUT "https://api.github.com/repos/${OWNER}/${REPO}/pages" -d '{"build_type":"workflow"}') || true
     echo "更新返回码: $CODE" ;;
esac
[ "$CODE" = "201" ] || [ "$CODE" = "204" ] || head -c 300 /tmp/pages.json; echo

if [ "$REPO" = "${OWNER}.github.io" ]; then URL="https://${OWNER}.github.io"; else URL="https://${OWNER}.github.io/${REPO}"; fi
echo "正式地址（等 GitHub 构建完 1-2 分钟才能打开）："
echo "  ${URL}"
