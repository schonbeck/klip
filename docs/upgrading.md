# Upgrading from older versions

## Coming from 0.1.1 or 0.1.0

0.1.1 and earlier used an app-grid entry that opened the settings, and
0.1.0 also quit when that window closed. 0.1.2 starts a screenshot from the
app grid instead, and rewrites the per-user `.desktop` file on every launch
(not only when the background klip starts). After upgrading:

```sh
pkill -x klip
rm -f ~/.local/share/applications/io.github.schonbeck.klip.desktop
klip
```

The last `klip` returns straight away, rewrites the app-grid entry, and
leaves klip in the tray. If the icon in the app grid does nothing after a
Homebrew upgrade, the leftover entry still named a removed `Cellar/klip/0.1.0`
path; removing that file and running `klip` once (as above) fixes it.

0.1.0 also ran the background klip inside the terminal that started it, and
its autostart entry named the versioned Cellar path. 0.1.1 already fixed
those: the background klip is detached, and autostart uses `<prefix>/bin/klip`.

## Coming from a pre-release build

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
