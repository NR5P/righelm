#!/bin/sh
# A private headless GNOME Shell with a virtual monitor much bigger than a
# 1920x1080 window (so it is not maximized); runs "$@" inside it.
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
gnome-shell --headless --wayland --no-x11 --virtual-monitor 2560x1600 --wayland-display righelm-demo > "${SHELL_LOG:-/dev/null}" 2>&1 &
SHELL_PID=$!
for i in $(seq 1 50); do [ -S "$XDG_RUNTIME_DIR/righelm-demo" ] && break; sleep 0.2; done
sleep 2
WAYLAND_DISPLAY=righelm-demo "$@"
STATUS=$?
kill $SHELL_PID
wait $SHELL_PID 2>/dev/null
exit $STATUS
