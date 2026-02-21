fn main() {
    println!(
        "cargo:rustc-env=BUILD_TIMESTAMP={}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    println!("cargo:rustc-env=BUILD_GIT_HASH={}", git_hash());
}

fn git_hash() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
        .trim()
        .to_string()
}
