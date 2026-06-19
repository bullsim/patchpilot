#!/usr/bin/env bash
# Schedule a daily headless PatchPilot run on macOS via launchd (LaunchAgent,
# runs in your user session so the admin prompt can appear).
#
#   ./install-schedule-macos.sh            # 03:00 daily, All
#   ./install-schedule-macos.sh 02 30 Software
#   ./install-schedule-macos.sh --remove
set -euo pipefail

LABEL="com.bullers.patchpilot.daily"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"

if [ "${1:-}" = "--remove" ]; then
  launchctl unload "$PLIST" 2>/dev/null || true
  rm -f "$PLIST"
  echo "Removed $LABEL"
  exit 0
fi

HOUR="${1:-3}"; MIN="${2:-0}"; MODE="${3:-All}"
APP="/Applications/PatchPilot.app/Contents/MacOS/patchpilot"
[ -x "$APP" ] || { echo "PatchPilot.app not found in /Applications — install it first."; exit 1; }

mkdir -p "$HOME/Library/LaunchAgents"
cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key>
  <array><string>$APP</string><string>--silent</string><string>--mode</string><string>$MODE</string></array>
  <key>StartCalendarInterval</key>
  <dict><key>Hour</key><integer>$HOUR</integer><key>Minute</key><integer>$MIN</integer></dict>
</dict></plist>
EOF

launchctl unload "$PLIST" 2>/dev/null || true
launchctl load "$PLIST"
echo "Scheduled $LABEL daily at $HOUR:$(printf '%02d' "$MIN") (mode: $MODE)"
