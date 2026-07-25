use std::{env, process::Command};

fn main() {
    println!("cargo::rerun-if-env-changed=DIFFO_BUILD_SHA");
    track_git_path("HEAD");
    track_git_path("packed-refs");
    if let Some(reference) = git_output(&["symbolic-ref", "HEAD"]) {
        track_git_path(&reference);
    }

    let sha = env::var("DIFFO_BUILD_SHA")
        .ok()
        .filter(|sha| !sha.is_empty())
        .or_else(|| git_output(&["rev-parse", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_owned());

    println!("cargo::rustc-env=DIFFO_BUILD_SHA={sha}");
}

fn track_git_path(path: &str) {
    if let Some(path) = git_output(&["rev-parse", "--git-path", path]) {
        println!("cargo::rerun-if-changed={path}");
    }
}

fn git_output(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
}
