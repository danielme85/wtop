use std::process::Command;

fn main() {
    // Git commit hash (short) — prefer env var (for Docker builds), fall back to git command
    let git_hash = std::env::var("GIT_HASH")
        .ok()
        .filter(|s| !s.is_empty() && s != "unknown")
        .unwrap_or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        });

    // Git branch name — prefer env var (for Docker builds), fall back to git command
    let git_branch = std::env::var("GIT_BRANCH")
        .ok()
        .filter(|s| !s.is_empty() && s != "unknown")
        .unwrap_or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        });

    // Build date (YYYY-MM-DD)
    let build_date = Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Target triple (e.g., x86_64-unknown-linux-gnu)
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=GIT_BRANCH={}", git_branch);
    println!("cargo:rustc-env=BUILD_DATE={}", build_date);
    println!("cargo:rustc-env=BUILD_TARGET={}", target);

    // Re-run if git HEAD or branch changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
}
