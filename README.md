# klip

A tiny, native Wayland region screenshot tool for Linux. Press a hotkey, drag a
rectangle, annotate if you like, and the PNG is on your clipboard.

Status: planning. Nothing to install yet.

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
