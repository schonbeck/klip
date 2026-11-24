# Development, packaging and releases

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

## Debugging and testing

- `KLIP_DEBUG=1 klip` prints debug info (version and git commit, screenshot
  and window size, scale, selection, toolbar position, shortcut setup).
  `klip --version` shows the commit it was built from.
- `klip --file some.png` uses an existing image instead of a screenshot (for
  testing without the portal).

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
