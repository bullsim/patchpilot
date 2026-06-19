#!/usr/bin/env bash
# Schedule a daily headless PatchPilot run on Linux (incl. Raspberry Pi) via a
# system systemd timer running as root (apt/snap/fwupd then need no prompt).
#
#   sudo ./install-schedule-linux.sh                 # 03:00 daily, All
#   sudo ./install-schedule-linux.sh 02:30 Software
#   sudo ./install-schedule-linux.sh --remove
set -euo pipefail
[ "$(id -u)" -eq 0 ] || { echo "Run with sudo."; exit 1; }

UNIT=patchpilot
SVC="/etc/systemd/system/$UNIT.service"
TIMER="/etc/systemd/system/$UNIT.timer"

if [ "${1:-}" = "--remove" ]; then
  systemctl disable --now "$UNIT.timer" 2>/dev/null || true
  rm -f "$SVC" "$TIMER"
  systemctl daemon-reload
  echo "Removed $UNIT timer"
  exit 0
fi

TIME="${1:-03:00}"; MODE="${2:-All}"
APP="$(command -v patchpilot || echo /usr/bin/patchpilot)"
[ -x "$APP" ] || { echo "patchpilot binary not found — install the .deb/AppImage first."; exit 1; }

cat > "$SVC" <<EOF
[Unit]
Description=PatchPilot daily update run
[Service]
Type=oneshot
ExecStart=$APP --silent --mode $MODE
EOF

cat > "$TIMER" <<EOF
[Unit]
Description=PatchPilot daily timer
[Timer]
OnCalendar=*-*-* $TIME:00
Persistent=true
[Install]
WantedBy=timers.target
EOF

systemctl daemon-reload
systemctl enable --now "$UNIT.timer"
echo "Scheduled $UNIT daily at $TIME (mode: $MODE)"
systemctl list-timers "$UNIT.timer" --no-pager || true
