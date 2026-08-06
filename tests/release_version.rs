//! Covers scripts/version.sh, the version arithmetic the release workflow runs.
//! The workflow reads the release version out of Cargo.toml, then bumps it once
//! the release is cut — so the arithmetic has to be exercised outside CI.

use std::path::{Path, PathBuf};
use std::process::Command;

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/version.sh")
}

fn run(args: &[&str]) -> (String, String, i32) {
    let out = Command::new(script())
        .args(args)
        .output()
        .expect("run script");
    (
        String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// A throwaway crate root holding just the two files the script rewrites.
fn fixture(name: &str, manifest_version: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gsd-status-version-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\n\
             name = \"gsd-status-ui\"\n\
             version = \"{manifest_version}\"\n\
             edition = \"2021\"\n\
             \n\
             [dependencies]\n\
             ratatui = \"0.30\"\n"
        ),
    )
    .expect("write fixture manifest");
    std::fs::write(
        dir.join("Cargo.lock"),
        format!(
            "[[package]]\n\
             name = \"crossterm\"\n\
             version = \"0.29.0\"\n\
             \n\
             [[package]]\n\
             name = \"gsd-status-ui\"\n\
             version = \"{manifest_version}\"\n\
             dependencies = [\n \"crossterm\",\n]\n"
        ),
    )
    .expect("write fixture lock");
    dir
}

#[test]
fn get_reads_the_package_version_from_the_manifest() {
    let dir = fixture("get", "0.6.0");
    let (stdout, stderr, code) = run(&["get", dir.to_str().unwrap()]);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "0.6.0");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn get_ignores_dependency_versions_that_precede_the_package_version() {
    // A naive `grep version` would pick up [dependencies] entries; the real
    // manifest also carries a [workspace] table above [package].
    let dir = fixture("get-deps", "0.6.0");
    let manifest = dir.join("Cargo.toml");
    let body = std::fs::read_to_string(&manifest).expect("read");
    std::fs::write(
        &manifest,
        format!("[workspace]\nmembers = [\"leaf-adapter\"]\n\n{body}"),
    )
    .expect("prepend workspace table");

    let (stdout, stderr, code) = run(&["get", dir.to_str().unwrap()]);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "0.6.0");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn next_bumps_the_minor_and_zeroes_the_patch() {
    for (from, want) in [("0.6.0", "0.7.0"), ("0.2.1", "0.3.0"), ("1.9.4", "1.10.0")] {
        let (stdout, stderr, code) = run(&["next", from]);
        assert_eq!(code, 0, "stderr={stderr}");
        assert_eq!(stdout, want, "next of {from}");
    }
}

#[test]
fn next_rejects_a_version_that_is_not_three_numeric_parts() {
    for bad in ["v0.6.0", "0.6", "0.6.0-rc1", "", "abc"] {
        let (_, stderr, code) = run(&["next", bad]);
        assert_ne!(code, 0, "{bad:?} must be rejected");
        assert!(
            stderr.contains(bad) || stderr.contains("version"),
            "error should name the bad input: {stderr}"
        );
    }
}

#[test]
fn set_rewrites_both_the_manifest_and_the_lock_entry() {
    let dir = fixture("set", "0.2.1");
    let (_, stderr, code) = run(&["set", "0.7.0", dir.to_str().unwrap()]);
    assert_eq!(code, 0, "stderr={stderr}");

    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).expect("read manifest");
    assert!(
        manifest.contains("version = \"0.7.0\""),
        "manifest must carry the new version:\n{manifest}"
    );
    assert!(
        !manifest.contains("0.2.1"),
        "old version must be gone:\n{manifest}"
    );
    assert!(
        manifest.contains("ratatui = \"0.30\""),
        "dependency versions must be untouched:\n{manifest}"
    );

    let lock = std::fs::read_to_string(dir.join("Cargo.lock")).expect("read lock");
    assert!(
        lock.contains("name = \"gsd-status-ui\"\nversion = \"0.7.0\""),
        "lock entry for the crate must follow the manifest:\n{lock}"
    );
    assert!(
        lock.contains("name = \"crossterm\"\nversion = \"0.29.0\""),
        "other lock entries must be untouched:\n{lock}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_is_idempotent() {
    let dir = fixture("set-idempotent", "0.7.0");
    let before = std::fs::read_to_string(dir.join("Cargo.toml")).expect("read");
    let (_, stderr, code) = run(&["set", "0.7.0", dir.to_str().unwrap()]);
    assert_eq!(code, 0, "stderr={stderr}");
    let after = std::fs::read_to_string(dir.join("Cargo.toml")).expect("read");
    assert_eq!(before, after, "setting the current version changes nothing");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_rejects_a_malformed_version() {
    let dir = fixture("set-bad", "0.6.0");
    let (_, _, code) = run(&["set", "v0.7", dir.to_str().unwrap()]);
    assert_ne!(code, 0, "malformed version must not be written");
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).expect("read");
    assert!(manifest.contains("version = \"0.6.0\""), "{manifest}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_script_and_cargo_agree_on_the_real_manifest_version() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (stdout, stderr, code) = run(&["get", root.to_str().unwrap()]);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, env!("CARGO_PKG_VERSION"));
}

fn semver(v: &str) -> (u64, u64, u64) {
    let mut parts = v
        .split('.')
        .map(|p| p.parse::<u64>().expect("numeric part"));
    (
        parts.next().expect("major"),
        parts.next().expect("minor"),
        parts.next().expect("patch"),
    )
}

#[test]
fn the_committed_version_is_ahead_of_the_last_published_release() {
    // main carries the version the *next* release will cut, and the release
    // workflow bumps it only after publishing — so no commit on main may share
    // a version number with a shipped build. Formula/gsd-status.rb is written
    // by that workflow and records what actually shipped.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let formula =
        std::fs::read_to_string(root.join("Formula/gsd-status.rb")).expect("read formula");
    let released = formula
        .lines()
        .find_map(|l| l.trim().strip_prefix("version \""))
        .and_then(|l| l.strip_suffix('"'))
        .expect("formula records a released version");

    let current = env!("CARGO_PKG_VERSION");
    assert!(
        semver(current) > semver(released),
        "Cargo.toml is {current} but v{released} is already published — \
         bump the manifest so dev builds do not reuse a released version"
    );
}
