//! Desktop integration files: the app's `.desktop` entry (launcher, needed for
//! portals and notifications to know the app) and the XDG autostart entry.

use crate::APP_ID;
use gtk::glib;
use std::io;
use std::path::{Path, PathBuf};

/// The running binary, by a path that survives upgrades: Homebrew runs
/// `<prefix>/Cellar/klip/<version>/bin/klip` through the `<prefix>/bin/klip`
/// symlink, and only the symlink stays valid after `brew upgrade`.
pub fn exe_path() -> PathBuf {
    let Ok(exe) = std::env::current_exe() else {
        return "klip".into();
    };
    stable_path(&exe).unwrap_or(exe)
}

/// `<prefix>/bin/<name>` for `<prefix>/Cellar/<formula>/<version>/bin/<name>`,
/// if it exists and points at the same file.
fn stable_path(exe: &Path) -> Option<PathBuf> {
    let s = exe.to_str()?;
    let (prefix, _) = s.split_once("/Cellar/")?;
    let candidate = Path::new(prefix).join("bin").join(exe.file_name()?);
    let same = candidate.canonicalize().ok()? == exe.canonicalize().ok()?;
    same.then_some(candidate)
}

/// The running binary, quoted for a desktop file `Exec=` line.
pub fn exe() -> String {
    let exe = exe_path().to_string_lossy().into_owned();
    if exe
        .chars()
        .any(|c| c.is_whitespace() || "\"'\\$`".contains(c))
    {
        let escaped: String = exe
            .chars()
            .flat_map(|c| match c {
                '"' | '`' | '$' | '\\' => vec!['\\', c],
                c => vec![c],
            })
            .collect();
        format!("\"{escaped}\"")
    } else {
        exe
    }
}

/// Whether the .deb (or a similar /usr install) shipped a desktop file.
/// Homebrew's copy in `<prefix>/share` doesn't count: GNOME often doesn't
/// have that directory in `XDG_DATA_DIRS`, so we still write a per-user
/// launcher with an absolute `Exec=` path.
fn distro_package_launcher() -> bool {
    ["/usr/share", "/usr/local/share"].iter().any(|d| {
        Path::new(d)
            .join("applications")
            .join(format!("{APP_ID}.desktop"))
            .exists()
    })
}

fn user_launcher_path() -> PathBuf {
    glib::user_data_dir()
        .join("applications")
        .join(format!("{APP_ID}.desktop"))
}

fn icon_in(dir: &Path) -> PathBuf {
    dir.join("icons/hicolor/scalable/apps")
        .join(format!("{APP_ID}.svg"))
}

/// `<prefix>/share` next to the binary's `bin/` (Homebrew's, /usr/share, ...).
fn own_share() -> Option<PathBuf> {
    let exe = exe_path();
    Some(exe.parent()?.parent()?.join("share"))
}

/// An icon theme directory with klip's icon that the desktop doesn't search
/// by itself (Homebrew's `<prefix>/share/icons` isn't in `XDG_DATA_DIRS`).
pub fn extra_icon_dir() -> Option<PathBuf> {
    let share = own_share()?;
    let searched = std::iter::once(glib::user_data_dir())
        .chain(glib::system_data_dirs())
        .any(|d| d == share);
    (!searched && icon_in(&share).exists()).then(|| share.join("icons"))
}

/// Whether klip's icon is in an icon theme dir every app searches (the .deb's
/// `/usr/share/icons`, or `~/.local/share/icons`).
pub fn themed_icon_installed() -> bool {
    std::iter::once(glib::user_data_dir())
        .chain(glib::system_data_dirs())
        .any(|d| icon_in(&d).exists())
}

/// klip's own icon if it's installed (the .deb and Homebrew ship it), else a
/// stock one. With Homebrew this relies on `extra_icon_dir` being added to
/// the icon theme's search path.
pub fn icon_name() -> &'static str {
    if themed_icon_installed() || extra_icon_dir().is_some() {
        APP_ID
    } else {
        "applets-screenshooter"
    }
}

/// `Icon=` for the desktop files: a file path if it's outside the theme dirs.
fn desktop_icon() -> String {
    match (extra_icon_dir(), own_share()) {
        (Some(_), Some(share)) => icon_in(&share).to_string_lossy().into_owned(),
        _ => icon_name().into(),
    }
}

/// Marks the desktop files klip writes itself (the packaged one differs).
const GENERATED_COMMENT: &str = "Region screenshot: select, annotate, copy or save";

fn entry(exec: &str, extra: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=klip\n\
         Comment={GENERATED_COMMENT}\n\
         Exec={exec}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Utility;Graphics;\n\
         {extra}",
        icon = desktop_icon()
    )
}

fn write_if_changed(path: PathBuf, content: &str) -> io::Result<()> {
    if std::fs::read_to_string(&path).is_ok_and(|old| old == content) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, content)
}

/// The app-grid / menu entry: starts a region capture. A "Settings" desktop
/// action (right-click on the icon or dock) opens the settings.
///
/// Called on every launch (not only in the background process) so a stale
/// 0.1.0 entry that opens settings, or names a removed Homebrew Cellar path,
/// is rewritten even if the background klip isn't running yet.
pub fn install_launcher() -> io::Result<()> {
    let path = user_launcher_path();
    if distro_package_launcher() {
        // The .deb already ships a launcher in `/usr/share`. A per-user copy
        // (from an earlier self-built or Homebrew klip) would hide it.
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        write_if_changed(path, &launcher_file(&exe(), &desktop_icon()))
    }
}

/// Contents of the app-grid `.desktop` file (`exec` is already quoted for
/// `Exec=`). The packaged file in `data/` is the same, with `Exec=klip`.
fn launcher_file(exec: &str, icon: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=klip\n\
         GenericName=Screenshot Tool\n\
         Comment={GENERATED_COMMENT}\n\
         Exec={exec}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Utility;Graphics;\n\
         Keywords=screenshot;capture;snip;clipboard;\n\
         StartupNotify=true\n\
         Actions=settings;\n\
         \n\
         [Desktop Action settings]\n\
         Name=Settings\n\
         Exec={exec} --settings\n"
    )
}

fn autostart_path() -> PathBuf {
    glib::user_config_dir()
        .join("autostart")
        .join(format!("{APP_ID}.desktop"))
}

/// Write or remove `~/.config/autostart/<app id>.desktop` (runs `klip --daemon`).
pub fn set_autostart(enabled: bool) -> io::Result<()> {
    let path = autostart_path();
    if enabled {
        let content = entry(
            &format!("{} --daemon", exe()),
            "NoDisplay=true\nX-GNOME-Autostart-enabled=true\n",
        );
        write_if_changed(path, &content)
    } else {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homebrew_cellar_path_maps_to_bin_symlink() {
        let root = std::env::temp_dir().join(format!("klip-test-{}", std::process::id()));
        let cellar = root.join("Cellar/klip/0.1.1/bin");
        std::fs::create_dir_all(&cellar).unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(cellar.join("klip"), "").unwrap();
        std::os::unix::fs::symlink("../Cellar/klip/0.1.1/bin/klip", root.join("bin/klip")).unwrap();
        assert_eq!(
            stable_path(&cellar.join("klip")),
            Some(root.join("bin/klip"))
        );
        // Not a Cellar path, or no symlink: keep the real path.
        assert_eq!(stable_path(&root.join("bin/klip")), None);
        std::fs::remove_file(root.join("bin/klip")).unwrap();
        assert_eq!(stable_path(&cellar.join("klip")), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn app_grid_launcher_starts_a_capture_and_has_a_settings_action() {
        let file = launcher_file("/opt/klip/bin/klip", "io.github.schonbeck.klip");
        let (entry, action) = file
            .split_once("[Desktop Action settings]\n")
            .expect("settings action");
        assert!(
            entry.contains("Exec=/opt/klip/bin/klip\n"),
            "app grid should start a capture, not settings: {file}"
        );
        assert!(
            !entry.contains("Exec=/opt/klip/bin/klip --settings"),
            "main Exec must not open settings: {file}"
        );
        assert!(file.contains("Actions=settings;"));
        assert!(action.contains("Exec=/opt/klip/bin/klip --settings\n"));
        assert!(file.contains("GenericName=Screenshot Tool"));
        assert!(file.contains(&format!("Comment={GENERATED_COMMENT}")));
    }

    #[test]
    fn packaged_desktop_file_matches_the_app_grid_behaviour() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data/io.github.schonbeck.klip.desktop");
        let file = std::fs::read_to_string(path).unwrap();
        let expected = launcher_file("klip", "io.github.schonbeck.klip");
        assert_eq!(file, expected);
    }
}
