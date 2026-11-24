<p align="center">
  <img src="data/io.github.schonbeck.klip.svg" width="96" alt="klip icon">
</p>

<h1 align="center">klip</h1>

<p align="center">
  <b>A tiny, native Wayland screenshot tool: drag, annotate, copy.</b>
</p>

<p align="center">
  <a href="https://github.com/schonbeck/klip/releases/latest"><img src="https://img.shields.io/github/v/release/schonbeck/klip" alt="Latest release"></a>
  <a href="https://github.com/schonbeck/klip/actions/workflows/ci.yml"><img src="https://github.com/schonbeck/klip/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/schonbeck/klip" alt="MIT license"></a>
</p>

<p align="center">
  <img src="docs/images/annotate.png" width="760" alt="klip: a selection with arrow, line, rectangle and ellipse annotations and the toolbar">
</p>

## ✨ Features

- ⌨️ **One shortcut**: press **Ctrl+Alt+P**, drag a rectangle, done.
- ✏️ **Annotate**: arrow, line, rectangle and ellipse, in any color.
- 📋 **Copy** to the clipboard or 💾 **Save** a PNG to `~/Pictures/Screenshots`.
- 🖥️ **Native Wayland** via the screenshot portal: GNOME first, also KDE and wlroots.
- 🧩 **Tray icon**, start at login, no setup questions.
- 🪶 **Small and fast**: one Rust + GTK4 binary.

## 📦 Install

**Homebrew** (Linux, prebuilt bottle):

```sh
brew install schonbeck/tap/klip
klip
```

**Ubuntu 26.04 / Debian** (amd64): get the `.deb` from the
[latest release](https://github.com/schonbeck/klip/releases/latest), then

```sh
sudo apt install ./klip_*_amd64.deb
klip
```

The first `klip` starts klip in the background (tray icon, shortcut, start at
login) and takes a screenshot. To build from source, see
[docs/development.md](docs/development.md).

## 🚀 Usage

Press **Ctrl+Alt+P** (or run `klip`, or click klip in the app grid). The
screen freezes; drag a rectangle and a toolbar appears:

| Action | Button | Keys |
| --- | --- | --- |
| Copy to the clipboard and close | **Copy** | Enter, Ctrl+C |
| Save a PNG and close | **Save** | Ctrl+S |
| Close without copying | **Cancel** | Esc |
| Select / Arrow / Line / Rectangle / Ellipse | ⬚ ↗ ╱ ▭ ◯ | S, A, L, R, E |
| Colors | swatches, color button | 1-4 |
| Undo | | Ctrl+Z |

<img src="docs/images/tray-menu.png" width="270" align="right" alt="klip's tray menu: Take Screenshot, Settings, About klip, Quit">

klip stays in the **tray**: click the icon for **Take Screenshot**,
**Settings**, **About klip** and **Quit**.

```sh
klip             # take a screenshot
klip --settings  # open the settings
klip --help      # all options
```

<br clear="right">

## ⚙️ Settings

<img src="docs/images/settings.png" width="480" alt="klip settings: save folder, start at login, screenshot shortcut">

Open them from the toolbar's ⚙, the tray, or `klip --settings`: the save
folder, start at login and the screenshot shortcut. Changes apply right away
and are stored in `~/.config/klip/config.ini`.

## 📚 More

- [How it works](docs/how-it-works.md): portals, the background klip, global
  shortcut (gsettings vs. GlobalShortcuts portal), permissions, known limits.
- [Upgrading](docs/upgrading.md) from older versions.
- [Development](docs/development.md): building, code layout, packaging,
  releases and Homebrew bottles.

## 📄 License

[MIT](LICENSE)
