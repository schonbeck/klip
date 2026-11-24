//! Desktop integration files: the app's `.desktop` entry (launcher, needed for
//! portals and notifications to know the app) and the XDG autostart entry.

use crate::APP_ID;
use gtk::glib;
use std::io;
use std::path::PathBuf;

/// The running binary, quoted for a desktop file `Exec=` line.
pub fn exe() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "klip".into());
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

/// Whether a system-wide (e.g. the .deb's) desktop file for klip exists.
fn system_launcher() -> bool {
    glib::system_data_dirs().iter().any(|d| {
        d.join("applications")
            .join(format!("{APP_ID}.desktop"))
            .exists()
    })
}

/// klip's own icon if it's installed (the .deb ships it), else a stock one.
pub fn icon_name() -> &'static str {
    let installed = std::iter::once(glib::user_data_dir())
        .chain(glib::system_data_dirs())
        .any(|d| {
            d.join("icons/hicolor/scalable/apps")
                .join(format!("{APP_ID}.svg"))
                .exists()
        });
    if installed {
        APP_ID
    } else {
        "applets-screenshooter"
    }
}

fn entry(exec: &str, extra: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=klip\n\
         Comment=Region screenshot: select, annotate, copy or save\n\
         Exec={exec}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Utility;Graphics;\n\
         {extra}",
        icon = icon_name()
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

/// `~/.local/share/applications/<app id>.desktop`: opens the settings; its
/// "Take Screenshot" action captures.
pub fn install_launcher() -> io::Result<()> {
    if system_launcher() {
        // Installed from a package, which ships its own.
        return Ok(());
    }
    let exe = exe();
    let content = entry(
        &format!("{exe} --settings"),
        &format!(
            "Actions=capture;\n\n\
             [Desktop Action capture]\n\
             Name=Take Screenshot\n\
             Exec={exe}\n"
        ),
    );
    let path = glib::user_data_dir()
        .join("applications")
        .join(format!("{APP_ID}.desktop"));
    write_if_changed(path, &content)
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
