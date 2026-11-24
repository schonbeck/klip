//! Settings, stored as a small keyfile in `~/.config/klip/config.ini`.

use gtk::glib;
use std::path::PathBuf;

pub const DEFAULT_SHORTCUT: &str = "<Super><Shift>s";
const GROUP: &str = "klip";

#[derive(Clone, Debug)]
pub struct Config {
    /// Where Save writes screenshots. `None` until the first Save asks.
    pub save_dir: Option<PathBuf>,
    /// Start `klip --daemon` at login (XDG autostart).
    pub autostart: bool,
    /// GTK accelerator, e.g. `<Super><Shift>s`.
    pub shortcut: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            save_dir: None,
            autostart: false,
            shortcut: DEFAULT_SHORTCUT.into(),
        }
    }
}

pub fn path() -> PathBuf {
    glib::user_config_dir().join("klip").join("config.ini")
}

impl Config {
    /// `None` if there is no config file yet (first run).
    pub fn load() -> Option<Self> {
        let kf = glib::KeyFile::new();
        kf.load_from_file(path(), glib::KeyFileFlags::NONE).ok()?;
        let string = |key| kf.string(GROUP, key).ok().map(|s| s.to_string());
        Some(Self {
            save_dir: string("save_dir")
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
            autostart: kf.boolean(GROUP, "autostart").unwrap_or(false),
            shortcut: string("shortcut")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_SHORTCUT.into()),
        })
    }

    pub fn save(&self) -> Result<(), glib::Error> {
        let path = path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let kf = glib::KeyFile::new();
        let dir = self.save_dir.as_ref().map(|d| d.to_string_lossy());
        kf.set_string(GROUP, "save_dir", dir.as_deref().unwrap_or(""));
        kf.set_boolean(GROUP, "autostart", self.autostart);
        kf.set_string(GROUP, "shortcut", &self.shortcut);
        kf.save_to_file(path)
    }
}
