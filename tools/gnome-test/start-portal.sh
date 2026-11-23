#!/bin/bash
# Workaround: graphical-session.target is not active (gnome-shell runs
# without gnome-session), so xdg-desktop-portal-gnome.service refuses to
# start. Run the backend by hand, then restart the frontend.
. ~/env.sh
pkill -x xdg-desktop-po 2>/dev/null
pgrep -f "^/usr/libexec/xdg-desktop-portal-gnome" >/dev/null || { nohup setsid /usr/libexec/xdg-desktop-portal-gnome -r > /tmp/xdpg.log 2>&1 < /dev/null & sleep 3; }
gdbus call --session -d org.freedesktop.DBus -o /org/freedesktop/DBus -m org.freedesktop.DBus.NameHasOwner org.freedesktop.impl.portal.desktop.gnome
systemctl --user stop xdg-desktop-portal; systemctl --user reset-failed xdg-desktop-portal
systemctl --user start xdg-desktop-portal; systemctl --user is-active xdg-desktop-portal
