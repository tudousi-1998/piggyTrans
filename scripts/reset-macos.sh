#!/usr/bin/env bash
# 彻底清理 PiggyTrans 在本机的残留与权限状态，用于「重装后快捷键闪退 / 权限失效」恢复。
# 用法：bash scripts/reset-macos.sh
set -e

BUNDLE_ID="com.piggytrans.app"
APP_PATH="/Applications/PiggyTrans.app"

echo "==> 1. 关闭所有 PiggyTrans 进程"
pkill -f piggytrans 2>/dev/null || true
pkill -f PiggyTrans 2>/dev/null || true
sleep 1

echo "==> 2. 重置辅助功能 / Apple Events 权限"
tccutil reset Accessibility "$BUNDLE_ID" 2>/dev/null || echo "   (Accessibility 没有现有记录)"
tccutil reset AppleEvents "$BUNDLE_ID" 2>/dev/null || echo "   (AppleEvents 没有现有记录)"
tccutil reset ListenEvent "$BUNDLE_ID" 2>/dev/null || true
tccutil reset PostEvent "$BUNDLE_ID" 2>/dev/null || true

echo "==> 3. 移除隔离属性"
if [ -d "$APP_PATH" ]; then
  xattr -dr com.apple.quarantine "$APP_PATH" 2>/dev/null || true
  echo "   已清理 $APP_PATH 的 quarantine"
else
  echo "   未在 /Applications 找到 PiggyTrans.app，跳过"
fi

echo "==> 4. 移除登录项中的旧 PiggyTrans LaunchAgent"
ls ~/Library/LaunchAgents/ 2>/dev/null | grep -i piggy | while read -r f; do
  echo "   备份并删除 ~/Library/LaunchAgents/$f"
  mv "$HOME/Library/LaunchAgents/$f" "$HOME/Library/LaunchAgents/$f.bak.$(date +%s)"
done

cat <<'EOF'

✓ 清理完成。请按下面顺序继续：

  1) 重启系统（推荐，确保 TCC 缓存生效）
  2) 重新打开 /Applications/PiggyTrans.app
  3) 按下全局快捷键 → 应弹出系统对话框，引导前往「隐私与安全性 → 辅助功能」勾选 PiggyTrans
  4) 勾选后再次按快捷键即可正常使用

若仍闪退，在终端运行下面命令抓日志并贴出：
  /Applications/PiggyTrans.app/Contents/MacOS/piggytrans
EOF
