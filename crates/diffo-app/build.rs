use std::{env, process::Command};

fn main() {
    println!("cargo::rerun-if-env-changed=DIFFO_RELEASE_VERSION");
    track_git_path("HEAD");
    track_git_path("packed-refs");
    if let Some(reference) = git_output(&["symbolic-ref", "HEAD"]) {
        track_git_path(&reference);
    }

    let tag = env::var("DIFFO_RELEASE_VERSION")
        .ok()
        .filter(|tag| !tag.is_empty())
        .or_else(|| git_output(&["describe", "--tags", "--exact-match", "HEAD"]))
        .unwrap_or_else(|| "dev".to_owned());
    let sha =
        git_output(&["rev-parse", "--short=7", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());

    println!("cargo::rustc-env=DIFFO_BUILD_TAG={tag}");
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
