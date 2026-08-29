use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let vv = Command::new(&rustc)
        .arg("-vV")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    let rustc_version = vv
        .lines()
        .find(|l| l.starts_with("rustc "))
        .unwrap_or("unknown")
        .trim()
        .to_string();
    let host = vv
        .lines()
        .find(|l| l.starts_with("host: "))
        .map(|l| l.trim_start_matches("host: ").trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    let commit = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    println!("cargo:rustc-env=ARTISAN_RUSTC_VERSION={rustc_version}");
    println!("cargo:rustc-env=ARTISAN_HOST_TRIPLE={host}");
    println!("cargo:rustc-env=ARTISAN_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=ARTISAN_GIT_DIRTY={dirty}");
    println!(
        "cargo:rustc-env=ARTISAN_OPT_LEVEL={}",
        std::env::var("OPT_LEVEL").unwrap_or_else(|_| "unknown".into())
    );
    println!(
        "cargo:rustc-env=ARTISAN_PROFILE={}",
        std::env::var("PROFILE").unwrap_or_else(|_| "unknown".into())
    );
}
