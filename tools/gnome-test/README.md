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
