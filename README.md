# klip

A tiny, native Wayland region screenshot tool for Linux. Press a hotkey, drag a
rectangle, annotate if you like, then Copy or Save.

Status: milestone 4 (capture, select, annotate, Copy / Save / Cancel,
settings, background mode with a global shortcut and tray icon). Packages:
a .deb on GitHub Releases and a Homebrew tap.

## What it does

- Region screenshot: drag a rectangle with the left mouse button held down.
- A toolbar next to the selection: annotation tools and colors, a gear for
  the settings, then **Copy**, **Save** and **Cancel**.
- **Copy** puts the PNG on the clipboard and closes. Nothing is copied
  unless you click Copy (or press Enter).
- **Save** writes the PNG to `~/Pictures/Screenshots` without asking (the
  folder can be changed in the settings) and closes.
- Annotate with arrows, lines and circles/ellipses, in a color you pick.
- No setup questions: klip starts with sensible defaults (save to
  `~/Pictures/Screenshots`, start at login, Ctrl+Alt+P), all changeable in
  the settings.
- Runs in the background (started at login) with a configurable global
  keyboard shortcut and a tray icon.
- PNG only. Small and simple on purpose.

## Approach

Works on GNOME Wayland (primary target, Ubuntu 26.04), and should also work on
KDE Plasma and wlroots compositors (Sway, Hyprland). No X11/XWayland tricks, and
no wlr-layer-shell or wlr-screencopy (GNOME supports neither).

1. **Capture**: call the xdg-desktop-portal `Screenshot` API non-interactively
   to grab the full screen as a PNG.
2. **Select**: show that frozen image in a fullscreen GTK4 window. The user
   drags a rectangle; Esc cancels.
3. **Annotate**: in the same window, a small toolbar with arrow, line, ellipse,
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

## Install

**Ubuntu 26.04 / Debian-based** (amd64): download `klip_<version>_amd64.deb`
from the [latest release](https://github.com/schonbeck/klip/releases/latest),
then

```sh
sudo apt install ./klip_*_amd64.deb
klip
```

**Homebrew on Linux**:

```sh
brew install schonbeck/tap/klip
klip
```

The first `klip` stays in the background (tray icon, global shortcut, start
at login). See [Try it](#try-it-ubuntu-2604-gnome-on-wayland) below.

## Build from source

Needs Rust 1.92 or newer (the GTK4 bindings require it; Ubuntu's own `cargo`
package may be older, so use [rustup](https://rustup.rs)) and the GTK 4
development files (GTK 4.12 or newer).

```sh
sudo apt install build-essential pkg-config libgtk-4-dev git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # if you have no Rust yet
git clone https://github.com/schonbeck/klip.git
cd klip
cargo build --release
# the binary is target/release/klip
```

## Try it (Ubuntu 26.04, GNOME on Wayland)

```sh
./target/release/klip
```

There are no setup questions, and the terminal is free again right away.
The first launch saves the default settings (`~/.config/klip/config.ini`),
turns on start at login and starts klip in the background (shortcut
Ctrl+Alt+P, tray icon), which then takes the screenshot. If you turn off
start at login in the settings, it stays off; `klip` still starts the
background klip for the current session (Quit in the tray stops it).

The screen freezes and dims. Drag a rectangle with the left mouse button; a
toolbar appears next to the selection:

| Action | Button | Keys |
| --- | --- | --- |
| Copy to the clipboard and close | **Copy** | Enter, Ctrl+C |
| Save to the screenshot folder and close | **Save** | Ctrl+S |
| Close without copying | **Cancel** | Esc |
| Select a new region / Arrow / Line / Ellipse | ⬚ ↗ ╱ ◯ | S, A, L, E |
| Colors | swatches, color button | 1-4 |
| Undo the last shape | | Ctrl+Z |
| Settings | ⚙ | |

- **Save** writes `klip-YYYY-MM-DD_HH-MM-SS.png` (with `-2` etc. if that
  exists) to `~/Pictures/Screenshots` (created if needed; the XDG pictures
  folder), with no dialog, and shows a "Screenshot saved" notification with
  the path. Change the folder in the settings.
- **Annotate**: pick Arrow, Line or Ellipse and drag inside the selection.
  Picking a color also switches from Select to Arrow. Annotations are baked
  into the copied or saved PNG.
- With the Select tool (the default), drag again to replace the selection
  (annotations are cleared).
- After **Copy** the image stays available because klip keeps running in
  the background: on Wayland the clipboard content is served by the app that
  copied it.

### Background mode, start at login, shortcut

```sh
klip               # take a screenshot (starts the background klip if needed)
klip --settings    # open the settings (likewise)
klip --daemon      # run the background klip in this terminal (for debugging;
                   # Ctrl+C stops it), or do nothing if it's already running
```

- **Settings** (gear in the toolbar, tray menu, `klip --settings`, or the
  "klip" entry in the app grid): the save folder (default
  `~/Pictures/Screenshots`), **Start at login** (default on) and the
  **screenshot shortcut** (click the button, press the new keys; Esc
  cancels). Changes apply right away. **About klip** shows the version
  (with git commit), license and website. Stored in `~/.config/klip/config.ini`.
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
  (opens the settings; right-click → "Take Screenshot"), unless a package
  (the .deb) installed one in `/usr/share/applications`. Homebrew's own copy
  lives in `<prefix>/share`, which GNOME doesn't search, so with Homebrew
  this is the one in the app grid. Desktops also use it for the app name in
  notifications and permission dialogs. Closing the settings window leaves
  the background klip running.
- **Tray icon** (background mode): click for the menu: Take Screenshot,
  Settings, About klip, Quit. It needs a StatusNotifierItem host, like Ubuntu's
  AppIndicator extension (on by default on Ubuntu) or KDE. Without one,
  klip runs without a tray icon.
- **Quit** in the tray menu stops the background klip. If klip uses the
  GNOME custom keybinding, the key keeps working (it starts klip again).

### Coming from 0.1.0

0.1.0 ran the background klip inside the terminal that started it (the
prompt didn't come back, and Ctrl+C or closing the terminal ended it), and
the app-grid entry opened the settings in a klip that quit when the window
closed. With Homebrew its autostart entry and app-grid entry also named the
versioned Cellar path. 0.1.1 fixes all of this, and rewrites both entries
the first time it runs, so after upgrading just run `pkill -x klip; klip`
once.

### Coming from an earlier build of this branch

An earlier build asked "Start klip automatically at login?" and saved the
answer, and earlier builds used Super+Shift+S as the default shortcut (now
Ctrl+Alt+P). Settings are saved with their values, so an existing
`config.ini` keeps its shortcut until you change it in the settings. To start
over with the defaults:

```sh
pkill -x klip
rm -f ~/.config/klip/config.ini ~/.config/autostart/io.github.schonbeck.klip.desktop
```

If you clicked **Add** in GNOME's "Add Keyboard Shortcuts" dialog back then,
GNOME keeps that entry, unused (klip now uses a custom keybinding on GNOME).
Remove it in Settings → Apps → klip, or with
`dconf reset -f /org/gnome/settings-daemon/global-shortcuts/io.github.schonbeck.klip/`.

### Permissions and other notes

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

Building from a tarball without `.git`: set `KLIP_GIT_HASH=<commit>` so
`--version` shows it. `tools/gnome-test/` has scripts for testing on a
headless GNOME Shell.

## Code layout

- `src/main.rs`: argument parsing and the single-instance GTK application.
- `src/app.rs`: what klip does with a command line: first-run question,
  background mode, shortcut registration, capture, clipboard lifetime.
- `src/ui.rs`: the fullscreen window, the toolbar (tools, colors, settings,
  Copy / Save / Cancel), keyboard shortcuts and saving.
- `src/settings.rs`: the settings window and the save-folder dialog.
- `src/config.rs`: `~/.config/klip/config.ini`.
- `src/desktop.rs`: the app's `.desktop` file and the autostart entry.
- `src/shortcut.rs`: GlobalShortcuts portal and GNOME custom keybinding.
- `src/tray.rs`: the tray icon (StatusNotifierItem via ksni).
- `src/annotate.rs`: annotation shapes, drawing them (cairo), baking them
  into the PNG.
- `src/capture.rs`: portal screenshot via ashpd (or `--file`).
- `src/selector.rs`: the fullscreen selection widget (drawing and dragging).
- `src/geometry.rs`: rectangle math, HiDPI and monitor mapping (unit tested).
- `src/output.rs`: crop, clipboard and PNG saving.
- `src/about.rs`: the About dialog.
- `data/`: desktop file and icon (installed by the packages).
- `packaging/`: Homebrew formula template and the formula bump script.

## Packaging

- **.deb**: `.github/workflows/release.yml` builds it with
  [cargo-deb](https://github.com/kornelski/cargo-deb) in an `ubuntu:26.04`
  container whenever a `vX.Y.Z` tag is pushed, and attaches
  `klip_X.Y.Z_amd64.deb`, the source tarball `klip-X.Y.Z.tar.gz` and
  `SHA256SUMS` to the GitHub Release. Built on Ubuntu 26.04, the package
  needs its library versions (glibc, GTK 4.12+). Pull requests that touch
  packaging build the same files as a workflow artifact.
- **Homebrew**: the formula lives in
  [schonbeck/homebrew-tap](https://github.com/schonbeck/homebrew-tap)
  (template: `packaging/homebrew/klip.rb`). It builds from the release source
  tarball, and `brew install` pours a prebuilt bottle (x86_64 Linux) when the
  tap has one. After a release, a PR in the tap runs `packaging/bump-formula.sh
  X.Y.Z Formula/klip.rb` (url and sha256). The tap's CI (`brew test-bot`)
  builds the bottle; adding the `pr-pull` label to that PR uploads it to a
  release of the tap, adds the `bottle do` block and lands the PR. The release
  workflow opens the bump PR automatically if the repository secret
  `HOMEBREW_TAP_TOKEN` is set (a fine-grained token for the tap with Contents
  and Pull requests read/write); otherwise the PR is made by hand.
- No PPA or apt repository yet. Later, optionally: Flatpak on Flathub. Snap
  is a poor fit (its sandbox makes screenshots and the clipboard awkward).

To release: bump `version` in `Cargo.toml`, merge, then
`git tag vX.Y.Z && git push origin vX.Y.Z`.

## Milestones

1. Capture via portal + fullscreen rectangle selection + clipboard copy.
2. Save / Cancel buttons and PNG save.
3. Annotation tools (arrow, line, ellipse) and color picker.
4. Copy button, settings, background mode with global shortcut, autostart, tray icon.
5. Release workflow: .deb on GitHub Releases, Homebrew tap formula.

## License

MIT
