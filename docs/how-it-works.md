# How klip works

Details behind the [README](../README.md): the capture flow, the background
klip, the global shortcut, desktop integration, permissions and known limits.

## Approach

Works on GNOME Wayland (primary target, Ubuntu 26.04), and should also work on
KDE Plasma and wlroots compositors (Sway, Hyprland). No X11/XWayland tricks, and
no wlr-layer-shell or wlr-screencopy (GNOME supports neither).

1. **Capture**: call the xdg-desktop-portal `Screenshot` API non-interactively
   to grab the full screen as a PNG.
2. **Select**: show that frozen image in a fullscreen GTK4 window. The user
   drags a rectangle; Esc cancels.
3. **Annotate**: in the same window, a small toolbar with arrow, line, rectangle, ellipse,
   color picker, Save and Cancel. Drawing happens on a GTK4 drawing area with
   cairo.
4. **Output**: Copy puts the cropped (and annotated) image on the clipboard as
   `image/png` via the GDK clipboard. Because Wayland drops clipboard content
   when the owning app exits, klip keeps running until something else takes
   the clipboard (or 10 minutes pass); normally klip runs in the background
   anyway.
   Save writes a PNG to the configured folder.

klip is a single-instance GTK application (`io.github.schonbeck.klip`). The
work is done by one background klip (`klip --daemon`). A plain `klip` (from
a terminal, the app grid or the shortcut) starts that background klip if it
isn't running, as a separate process detached from the terminal (its own
session, output in `~/.local/state/klip/klip.log`), hands it the request,
and exits right away.

### Global shortcut

- **GNOME (default there)**: a GNOME custom keybinding, set through
  `gsettings` (`org.gnome.settings-daemon.plugins.media-keys`, entry
  `custom-keybindings/klip/`), that runs `klip`. No dialog, and it works even
  when klip isn't running (a press then starts the background klip). You'll see
  it in Settings → Keyboard → View and Customize Shortcuts → Custom
  Shortcuts.
- **Other desktops** (KDE, Hyprland, ...): the xdg-desktop-portal
  `GlobalShortcuts` API, while klip runs in the background. If the portal is
  missing or fails, klip falls back to the GNOME keybinding when available.

Why not the portal on GNOME: GNOME 50 supports it, but the first time shows
an "Add Keyboard Shortcuts" confirmation dialog, then keeps its own copy of
the key and ignores later changes from the app (its portal has no "change
shortcut" call yet). The custom keybinding has neither problem. Even with
the portal, klip switches to the keybinding if the desktop keeps a different
key than the one set in klip's settings. `KLIP_SHORTCUT_BACKEND=portal` or
`=gnome` forces one way (for testing).

## Stack

**Rust + GTK4** (gtk4-rs), with [ashpd](https://github.com/bilelmoussaoui/ashpd)
for the portals and [ksni](https://github.com/iovxw/ksni) for the tray icon
(plain D-Bus, no GTK3). One small, fast-starting binary (important for a hotkey tool),
no runtime to ship, and straightforward to package.

## Capture, Copy and Save in detail

There are no setup questions, and the terminal is free again right away.
The first launch saves the default settings (`~/.config/klip/config.ini`),
turns on start at login and starts klip in the background (shortcut
Ctrl+Alt+P, tray icon), which then takes the screenshot. If you turn off
start at login in the settings, it stays off; `klip` still starts the
background klip for the current session (Quit in the tray stops it).

- **Save** writes `klip-YYYY-MM-DD_HH-MM-SS.png` (with `-2` etc. if that
  exists) to `~/Pictures/Screenshots` (created if needed; the XDG pictures
  folder), with no dialog, and shows a "Screenshot saved" notification with
  the path. Change the folder in the settings.
- **Annotate**: pick Arrow, Line, Rectangle (outline) or Ellipse and drag inside the selection.
  Picking a color also switches from Select to Arrow. Annotations are baked
  into the copied or saved PNG.
- With the Select tool (the default), drag again to replace the selection
  (annotations are cleared).
- After **Copy** the image stays available because klip keeps running in
  the background: on Wayland the clipboard content is served by the app that
  copied it.

## Background mode, start at login, shortcut

```sh
klip               # take a screenshot (starts the background klip if needed)
klip --settings    # open the settings (likewise)
klip --daemon      # run the background klip in this terminal (for debugging;
                   # Ctrl+C stops it), or do nothing if it's already running
```

- **Settings** (gear in the toolbar, tray menu, `klip --settings`, or
  right-click the app-grid / dock icon → Settings): the save folder (default
  `~/Pictures/Screenshots`), **Start at login** (default on) and the
  **screenshot shortcut** (click the button, press the new keys; Esc
  cancels). Changes apply right away. **About klip** shows the version
  (with git commit), license and website. Stored in `~/.config/klip/config.ini`.
  Opening klip from the app grid starts a screenshot, like a plain `klip`
  or the shortcut; after Copy, Save or Esc the background klip stays in
  the tray. Closing the settings window does the same.
- **Start at login** writes `~/.config/autostart/io.github.schonbeck.klip.desktop`
  (runs `klip --daemon`); turning it off removes the file. The first launch
  writes it silently (only when there's no config file yet), and the
  background klip rewrites it on start, so it always names the current
  binary (with Homebrew `<prefix>/bin/klip`, not the versioned
  `Cellar/klip/<version>/bin/klip` that `brew upgrade` removes). This uses XDG
  autostart rather than a systemd user unit: it works on every desktop,
  needs no `systemctl` calls, only runs in a graphical session (which a
  screenshot tool needs anyway), and Settings → Apps lists it as a startup
  app on GNOME.
- klip also installs `~/.local/share/applications/io.github.schonbeck.klip.desktop`
  (starts a screenshot; right-click → Settings), unless a package (the .deb)
  installed one in `/usr/share/applications`. Homebrew's own copy lives in
  `<prefix>/share`, which GNOME often doesn't search, so with Homebrew this
  per-user file is the one in the app grid, with an absolute `Exec=` path.
  It is rewritten on every launch, so a leftover entry from an older version
  (one that opened the settings, or named a removed Homebrew Cellar path)
  is replaced. Desktops also use it for the app name in notifications and
  permission dialogs.
- **Tray icon** (background mode): click for the menu: Take Screenshot,
  Settings, About klip, Quit. It needs a StatusNotifierItem host, like Ubuntu's
  AppIndicator extension (on by default on Ubuntu) or KDE. Without one,
  klip runs without a tray icon. The icon is embedded in the binary (16-64 px
  pixmaps rendered from `data/io.github.schonbeck.klip.svg` into
  `data/tray/`), so it shows however klip was installed; with the .deb the
  tray also gets the themed icon name.
- **Quit** in the tray menu stops the background klip. If klip uses the
  GNOME custom keybinding, the key keeps working (it starts klip again).

## Permissions and other notes

- **Screenshot permission**: GNOME's screenshot portal asks "Allow klip to
  Take Screenshots?" the first time an app uses it. klip registers with the
  portal under its own app id (`io.github.schonbeck.klip`; needed for the
  portal shortcut and for notifications), and older builds ran as an unnamed
  app, which is why a new question appeared. To avoid the dialog, klip
  records "yes" for its app id in the portal permission store before its
  first screenshot (`org.freedesktop.impl.portal.PermissionStore`, table
  `screenshot`). That's the same entry GNOME's dialog writes and Settings →
  Privacy → Screenshots shows. An existing answer, including "no", is left
  alone. If the screenshot still fails (GNOME Shell can't show its question
  while no window has focus), klip shows "Could not take a screenshot" with
  **Try Again**, and the retry brings up GNOME's question.
- `KLIP_DEBUG=1 klip` prints debug info (version and git commit, screenshot
  and window size, scale, selection, toolbar position, shortcut setup).
  `klip --version` shows the commit it was built from.
- `klip --file some.png` uses an existing image instead of a screenshot (for
  testing without the portal).
- GNOME plays a shutter sound and flashes the screen with each screenshot
  (see known limits).
- The portal saves its screenshot as a file (on GNOME in
  `~/Pictures/Screenshots`); klip deletes that file after loading it, so the
  folder doesn't fill up with full-screen shots.

## Known limits

- **Shutter sound and flash on GNOME**: GNOME's screenshot portal always asks
  GNOME Shell for a screenshot *with* flash, and Shell plays the
  `screen-capture` sound with it. The portal has no option to turn either off,
  and Shell's own screenshot D-Bus API only accepts calls from the portal and
  GNOME's media-keys daemon. The sound obeys the system "event sounds"
  setting: `gsettings set org.gnome.desktop.sound event-sounds false` silences
  it, together with all other system sounds. The flash can't be turned off.

- **Multi-monitor**: the selection window covers only the monitor it opens on
  (usually the one with the focused window). klip works out that monitor's
  part of the portal's all-monitors screenshot from the monitor layout; with
  monitors at *different* scale factors that mapping may be slightly off.
- Annotations can't be moved or edited after drawing (only undone), and the
  stroke width is fixed.
- The selection can't be resized after release; drag a new one instead.
- If the copy is taken over by a clipboard manager, klip exits right away
  (which is fine: the manager then holds the image).
