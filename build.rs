//! Embed the git commit in the version (`klip --version`, debug log).
fn main() {
    let hash = std::process::Command::new("git")
        .args(["describe", "--always", "--dirty", "--exclude", "*"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=KLIP_GIT_HASH={hash}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-changed=.git/index");
}
