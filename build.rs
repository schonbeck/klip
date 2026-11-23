//! Embed the git commit in the version (`klip --version`, debug log).
/// Set `KLIP_GIT_HASH` to override (e.g. when building from a tarball).
fn main() {
    println!("cargo:rerun-if-env-changed=KLIP_GIT_HASH");
    let from_env = std::env::var("KLIP_GIT_HASH")
        .ok()
        .filter(|s| !s.is_empty());
    let hash = from_env.or_else(|| {
        std::process::Command::new("git")
            .args(["describe", "--always", "--dirty", "--exclude", "*"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    });
    let hash = hash.unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=KLIP_GIT_HASH={hash}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-changed=.git/index");
}
