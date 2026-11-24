//! Starting the background klip (`klip --daemon`) as a detached process.
//!
//! A plain `klip` (terminal, app grid, keyboard shortcut) must never become
//! the background klip itself: from a terminal that would tie the daemon to
//! the shell (it blocks the prompt, and Ctrl+C or closing the terminal kills
//! it). Instead it starts `klip --daemon` in a new session with its output
//! in a log file, waits until it owns klip's D-Bus name, and then forwards
//! its request to it like any later `klip` does.

use crate::APP_ID;
use gtk::{gio, glib};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long to wait for a freshly started daemon to show up on the bus.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// `~/.local/state/klip/klip.log`: the detached daemon's stdout and stderr.
pub fn log_path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| glib::home_dir().join(".local/state"))
        .join("klip")
        .join("klip.log")
}

fn session_bus() -> Option<gio::DBusConnection> {
    gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()
}

/// Whether some klip process owns the application's D-Bus name.
fn running(bus: &gio::DBusConnection) -> bool {
    bus.call_sync(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        Some(&(APP_ID,).into()),
        Some(glib::VariantTy::new("(b)").unwrap()),
        gio::DBusCallFlags::NONE,
        1000,
        gio::Cancellable::NONE,
    )
    .ok()
    .and_then(|v| v.get::<(bool,)>())
    .is_some_and(|(b,)| b)
}

/// Start the detached daemon unless one is running, and wait for it. On any
/// problem (no session bus, spawn failure, timeout) this process just carries
/// on and runs the request itself, as before.
pub fn ensure_running() {
    let Some(bus) = session_bus() else {
        crate::debug!("no session bus: not starting a background klip");
        return;
    };
    if running(&bus) {
        return;
    }
    let mut child = match spawn() {
        Ok(child) => child,
        Err(err) => {
            eprintln!("klip: could not start the background klip: {err}");
            return;
        }
    };
    crate::debug!("started the background klip (pid {})", child.id());
    let start = Instant::now();
    while start.elapsed() < STARTUP_TIMEOUT {
        if running(&bus) {
            return;
        }
        if let Ok(Some(status)) = child.try_wait() {
            eprintln!(
                "klip: the background klip exited ({status}); see {}",
                log_path().display()
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!("klip: the background klip didn't start in time");
}

fn spawn() -> std::io::Result<std::process::Child> {
    let log = log_path();
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Keep only the latest daemon's output.
    let out = std::fs::File::create(&log)?;
    let err = out.try_clone()?;
    let mut cmd = Command::new(crate::desktop::exe_path());
    cmd.arg("--daemon")
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        // Not our desktop-launch token: that one is for the window the
        // request below opens, and is forwarded with it.
        .env_remove("XDG_ACTIVATION_TOKEN")
        .env_remove("DESKTOP_STARTUP_ID");
    // A new session: no controlling terminal, so no SIGINT from Ctrl+C or
    // SIGHUP when the terminal closes.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn()
}
