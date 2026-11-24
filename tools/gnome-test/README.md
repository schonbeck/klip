# Testing klip on headless GNOME

Scripts used to test klip end to end on a headless GNOME Shell
(`gnome-shell --headless --virtual-monitor 1920x1080`, Ubuntu 26.04, GNOME 50)
with no real input devices.

- `env.sh`: session environment (`XDG_RUNTIME_DIR`, D-Bus, `WAYLAND_DISPLAY`).
- `start-portal.sh`: starts `xdg-desktop-portal-gnome` by hand. Needed when
  gnome-shell runs without gnome-session: `graphical-session.target` is then
  inactive and the backend's systemd unit refuses to start (`Requisite=`), so
  the portal falls back to gtk and has no Screenshot interface. Also run
  `dbus-update-activation-environment --systemd XDG_CURRENT_DESKTOP=GNOME`
  first if the user manager lacks it.
- `shot.py`: one non-interactive portal screenshot, prints the file URI.
- `e2e.py`: drives a test through Mutter's RemoteDesktop D-Bus API (virtual
  keyboard and clipboard read) plus the Screenshot portal. It takes a JSON
  list of steps, for example:

  ```sh
  python3 e2e.py '[["key","shift"],["wait",1],
    ["klip","600,300,500,300","/tmp/k.log"],["wait",5],["shot","toolbar"],
    ["clip","clipboard"],["key","ret"],["wait",4],["shot","dialog"],
    ["key","ret"],["wait",3],["status"],["copytext"],["wait",3],["status"]]'
  ```

  Screenshots and the clipboard PNG go to `~/proof/`. The first virtual key
  after a session starts is swallowed (keymap setup), hence the leading
  `shift`. `klip` steps run `target/release/klip` with `KLIP_DEBUG=1` and
  `KLIP_TEST_SELECT`. Other steps: `["key","a"]` (any single character or
  `esc`/`ret`/`shift`), `["ctrlkey","z"]`, `["drag",x0,y0,x1,y1]` (virtual
  pointer, left button).

Non-interactive screenshots need the permission GNOME asks for on first use
("Allow Apps to Take Screenshots?"). Without input it can be granted directly
in the permission store (for unsandboxed apps the app id is empty):

```sh
gdbus call --session -d org.freedesktop.impl.portal.PermissionStore \
  -o /org/freedesktop/impl/portal/PermissionStore \
  -m org.freedesktop.impl.portal.PermissionStore.SetPermission \
  screenshot true screenshot "" '["yes"]'
```

If the dialog was already shown and nobody answered, it stays on screen;
restarting gnome-shell clears it. The session starts in the Overview; leave it
with `gdbus call --session -d org.gnome.Shell -o /org/gnome/Shell -m
org.freedesktop.DBus.Properties.Set org.gnome.Shell OverviewActive "<false>"`.

## Milestone 4 notes

- `e2e.py` steps added: `["klip", sel, log, [args...]]` (e.g. `["--daemon"]`),
  `["combo", ["super","shift","s"]]`, `["click", x, y]`, `["sh", "cmd"]`.
  `E2E_KEEP=1` leaves started klip processes running (for a daemon).
- The headless session doesn't start `gsd-media-keys`; GNOME custom
  keybindings only fire with it running:
  `setsid nohup /usr/libexec/gsd-media-keys >/tmp/gsd.log 2>&1 &`.
  The GlobalShortcuts portal also goes through Shell's GrabAccelerators.
- Tray icon: `gnome-extensions enable ubuntu-appindicators@ubuntu.com`
  (installed, but off in this container) provides the StatusNotifierWatcher.
- klip registers with the portal as `io.github.schonbeck.klip` and records
  "yes" for that id in the permission store before its first screenshot (if
  there's no answer yet). To test from a clean state:
  `busctl --user call org.freedesktop.impl.portal.PermissionStore /org/freedesktop/impl/portal/PermissionStore org.freedesktop.impl.portal.PermissionStore DeletePermission sss screenshot screenshot io.github.schonbeck.klip`.
  If GNOME has to ask, Shell 50 can't show that dialog without a focused app
  (`focus_app is null` in the journal); klip's "Try Again" window handles that.
- Fresh config: `rm -rf ~/.config/klip ~/.config/autostart/io.github.schonbeck.klip.desktop`,
  and `gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "[]"`.
