# klip

A tiny, native Wayland region screenshot tool for Linux. Press a hotkey, drag a
rectangle, annotate if you like, and the PNG is on your clipboard.

Status: milestone 2 (capture, select, copy to clipboard, Save / Cancel). No
packages yet; build from source.

## What it does

- Region screenshot: drag a rectangle with the left mouse button held down.
- The PNG is copied to the clipboard automatically after the selection.
- Clearly visible **Save** (writes a PNG file) and **Cancel** buttons.
- Annotate with arrows, lines and circles/ellipses, in a color you pick.
- A configurable global keyboard shortcut starts a region screenshot.
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
4. **Output**: the cropped (and annotated) image is put on the clipboard as
   `image/png` via the GDK clipboard. Because Wayland drops clipboard content
   when the owning app exits, klip stays alive in the background until
   something else takes the clipboard (or a timeout passes). Save writes a PNG
   through a file dialog (portal FileChooser).

### Global shortcut

- **Default on GNOME**: `klip --install-shortcut "<Super><Shift>s"` registers a
  GNOME custom keybinding with `gsettings` that runs `klip`. Documented manual
  `gsettings` commands too.
- **Portal route**: the xdg-desktop-portal `GlobalShortcuts` API (GNOME 48+,
  KDE, Hyprland). Needs klip running as a small background service, so it is
  optional (`klip --daemon`).

## Stack

**Rust + GTK4** (gtk4-rs), with [ashpd](https://github.com/bilelmoussaoui/ashpd)
for the portals. One small, fast-starting binary (important for a hotkey tool),
no runtime to ship, and straightforward to package.

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

The screen freezes and dims. Drag a rectangle with the left mouse button; when
you let go, the region is **already on the clipboard** and a toolbar appears
next to the selection:

- **Save** (or Enter, Ctrl+S): pick a file name (default
  `~/Pictures/klip-YYYY-MM-DD_HH-MM-SS.png`) and write the PNG. The window
  then closes; the clipboard copy stays.
- **Cancel** (or Esc): close the window. The clipboard copy stays (see below).
- Drag again to replace the selection (it's copied again).
- Esc before selecting anything exits without touching the clipboard.

- `klip` keeps running in the background after the window closes, because on Wayland the
  clipboard content is served by the app that copied it. It exits as soon as
  something else is copied, or after 10 minutes.
- `klip --file some.png` uses an existing image instead of a screenshot (for
  testing without the portal).
- To try it on a hotkey before milestone 4: Settings → Keyboard → View and
  Customize Shortcuts → Custom Shortcuts, command `/full/path/to/klip`.
- GNOME may ask once whether klip may take screenshots; allow it.
- The portal saves its screenshot as a file (on GNOME in
  `~/Pictures/Screenshots`); klip deletes that file after loading it, so the
  folder doesn't fill up with full-screen shots.

## Known limits (milestone 1)

- **Multi-monitor**: the selection window covers only the monitor it opens on
  (usually the one with the focused window). klip works out that monitor's
  part of the portal's all-monitors screenshot from the monitor layout; with
  monitors at *different* scale factors that mapping may be slightly off.
- No annotations yet (milestone 3).
- Cancel after a selection does not undo the clipboard copy (it was made the
  moment you let go). Restoring the previous clipboard isn't possible in
  general on Wayland, and "copy, then just close" is the common case.
- The selection can't be resized after release; drag a new one instead.
- If the copy is taken over by a clipboard manager, klip exits right away
  (which is fine: the manager then holds the image).

## Code layout

- `src/main.rs`: startup and argument parsing.
- `src/ui.rs`: the fullscreen window, the Save / Cancel toolbar (with an empty
  `tools` box where the milestone 3 annotation buttons go), keyboard
  shortcuts and the file dialog.
- `src/capture.rs`: portal screenshot via ashpd (or `--file`).
- `src/selector.rs`: the fullscreen selection widget (drawing and dragging).
- `src/geometry.rs`: rectangle math, HiDPI and monitor mapping (unit tested).
- `src/output.rs`: crop, clipboard, PNG saving, and staying alive while we
  own the clipboard.

## Packaging plan

- **.deb** built in GitHub Actions and attached to GitHub Releases; later a PPA
  so `apt install klip` works on Ubuntu.
- **Homebrew on Linux** via a tap (`schonbeck/homebrew-tap`).
- Later, optionally: Flatpak on Flathub. Snap is a poor fit (sandbox makes
  screenshots and clipboard awkward).

Nothing is published anywhere public without the owner's explicit OK.

## Milestones

1. Capture via portal + fullscreen rectangle selection + clipboard copy.
2. Save / Cancel buttons and PNG save.
3. Annotation tools (arrow, line, ellipse) and color picker.
4. Global shortcut (gsettings installer + optional GlobalShortcuts portal).
5. CI builds, .deb, Homebrew tap formula.

## License

MIT
