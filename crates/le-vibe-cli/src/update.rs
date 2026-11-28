use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::settings::{arg_value, self_repo_path};

const DEFAULT_REPO_SLUG: &str = "amirulaliffcherahim/le-vibe";

fn bin_name() -> &'static str {
    if cfg!(windows) {
        "le-vibe.exe"
    } else {
        "le-vibe"
    }
}

fn target_install_path(args: &[String]) -> PathBuf {
    if let Some(custom) = arg_value(args, "--target") {
        return PathBuf::from(custom);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(name) = exe.file_name().and_then(|s| s.to_str())
        && name.eq_ignore_ascii_case(bin_name())
    {
        return exe;
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".cargo").join("bin").join(bin_name())
}

/// Remove leftover `*.old.<ts>` sidecar backups left by a previous hot-swap.
pub(crate) fn cleanup_stale_old_binaries(exe: &Path) {
    let Some(dir) = exe.parent() else {
        return;
    };
    let Some(fname) = exe.file_name().and_then(|s| s.to_str()) else {
        return;
    };
    let prefix = format!("{fname}.old.");
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if let Some(s) = name.to_str()
            && s.starts_with(&prefix)
        {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Atomically replace `dst` with `src` even if `dst` is a currently running Windows executable.
/// On Windows, running `.exe` files cannot be overwritten in-place, but they CAN be renamed
/// out of the way in the same directory; the new binary then moves into `dst` cleanly.
pub(crate) fn hot_swap_binary(src: &Path, dst: &Path) -> io::Result<Option<PathBuf>> {
    if !src.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("source binary not found: {}", src.display()),
        ));
    }
    let parent = dst.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "target binary has no parent dir")
    })?;
    fs::create_dir_all(parent)?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let fname = dst
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(bin_name());

    // Stage in the same directory so final rename is atomic on the same volume.
    let staged = parent.join(format!("{fname}.new.{ts}"));
    fs::copy(src, &staged)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&staged, fs::Permissions::from_mode(0o755));
    }

    let backup = if dst.exists() {
        let old = parent.join(format!("{fname}.old.{ts}"));
        if let Err(err) = fs::rename(dst, &old) {
            let _ = fs::remove_file(&staged);
            return Err(io::Error::new(
                err.kind(),
                format!("failed to park active binary {}: {err}", dst.display()),
            ));
        }
        Some(old)
    } else {
        None
    };

    if let Err(err) = fs::rename(&staged, dst) {
        // Roll back parked binary if final swap fails.
        if let Some(ref old) = backup {
            let _ = fs::rename(old, dst);
        }
        let _ = fs::remove_file(&staged);
        return Err(io::Error::new(
            err.kind(),
            format!("failed to move staged binary into {}: {err}", dst.display()),
        ));
    }

    // Best-effort delete backup; if another process still holds it (Windows),
    // `cleanup_stale_old_binaries` will sweep it on the next run.
    let leftover = backup.filter(|old| fs::remove_file(old).is_err());
    Ok(leftover)
}

fn download_release_binary(tag: Option<&str>, out_path: &Path) -> Result<String, String> {
    let repo = std::env::var("LE_VIBE_RELEASE_REPO")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_REPO_SLUG.to_string());
    let asset = bin_name();

    // Prefer `gh release download` when authenticated (works for prereleases & private repos).
    let mut gh = Command::new("gh");
    gh.args(["release", "download"]);
    if let Some(t) = tag {
        gh.arg(t);
    }
    gh.args([
        "--repo",
        &repo,
        "--pattern",
        asset,
        "--output",
        &out_path.to_string_lossy(),
        "--clobber",
    ]);
    if let Ok(status) = gh.status()
        && status.success()
        && out_path.is_file()
    {
        return Ok(tag.unwrap_or("latest").to_string());
    }

    // Fallback to direct GitHub release URL via curl.
    let url = match tag {
        Some(t) => format!("https://github.com/{repo}/releases/download/{t}/{asset}"),
        None => format!("https://github.com/{repo}/releases/latest/download/{asset}"),
    };
    let status = Command::new("curl")
        .args(["-fsSL", &url, "-o", &out_path.to_string_lossy()])
        .status()
        .map_err(|e| format!("failed to invoke curl: {e}"))?;
    if !status.success() || !out_path.is_file() {
        return Err(format!("failed to download release asset from {url}"));
    }
    Ok(tag.unwrap_or("latest").to_string())
}

fn build_local_release(repo: &str, pull_main: bool) -> Result<PathBuf, String> {
    if pull_main {
        let pull = Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(repo)
            .status()
            .map_err(|e| format!("failed to run git pull: {e}"))?;
        if !pull.success() {
            return Err(format!("git pull failed (check uncommitted changes in {repo})"));
        }
    }
    let build = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--no-default-features",
            "-p",
            "le-vibe-cli",
        ])
        .current_dir(repo)
        .status()
        .map_err(|e| format!("failed to run cargo build: {e}"))?;
    if !build.success() {
        return Err("cargo build --release --no-default-features -p le-vibe-cli failed".to_string());
    }
    let built = PathBuf::from(repo)
        .join("target")
        .join("release")
        .join(bin_name());
    if !built.is_file() {
        return Err(format!("built binary missing at {}", built.display()));
    }
    Ok(built)
}

pub(crate) fn run_update(args: &[String]) {
    let dst = target_install_path(args);
    cleanup_stale_old_binaries(&dst);

    let from_release = args.iter().any(|a| a == "--release") || arg_value(args, "--tag").is_some();
    let local_only = args.iter().any(|a| a == "--local");
    let custom_bin = arg_value(args, "--from");

    let (src, temp_download): (PathBuf, Option<PathBuf>) = if let Some(path) = custom_bin {
        println!("Hot-swapping le-vibe from {path} -> {}...", dst.display());
        (PathBuf::from(path), None)
    } else if from_release {
        let tag = arg_value(args, "--tag");
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("le-vibe-dl-{ts}-{}", bin_name()));
        println!(
            "Downloading le-vibe release ({}) -> {}...",
            tag.as_deref().unwrap_or("latest"),
            dst.display()
        );
        match download_release_binary(tag.as_deref(), &tmp) {
            Ok(resolved) => {
                println!("Downloaded release {resolved}.");
                (tmp.clone(), Some(tmp))
            }
            Err(err) => {
                let _ = fs::remove_file(&tmp);
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    } else {
        let repo = self_repo_path();
        if local_only {
            println!("Building local le-vibe from {repo} (no git pull)...");
        } else {
            println!("Updating le-vibe from {repo}...");
        }
        match build_local_release(&repo, !local_only) {
            Ok(built) => (built, None),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    };

    let swap_res = hot_swap_binary(&src, &dst);
    if let Some(tmp) = temp_download {
        let _ = fs::remove_file(tmp);
    }

    match swap_res {
        Ok(leftover) => {
            println!("le-vibe updated at {} (no need to close active sessions).", dst.display());
            if let Some(old) = leftover {
                println!(
                    "Previous running binary parked at {} (cleaned automatically on next start).",
                    old.display()
                );
            }
            println!("To switch an active TUI session to the new binary, restart with: le-vibe -c");
        }
        Err(err) => {
            eprintln!("Failed to hot-swap le-vibe binary: {err}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hot_swap_replaces_existing_binary_and_cleans_stale_backups() {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("le-vibe-hotswap-test-{ts}"));
        fs::create_dir_all(&dir).unwrap();

        let src = dir.join("built.exe");
        let dst = dir.join("le-vibe.exe");
        fs::write(&dst, b"old-version").unwrap();
        fs::write(&src, b"new-version").unwrap();

        let leftover = hot_swap_binary(&src, &dst).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"new-version");
        assert!(leftover.is_none());

        // Simulate a parked Windows backup from a running process and verify sweep.
        let parked = dir.join("le-vibe.exe.old.12345");
        fs::write(&parked, b"parked").unwrap();
        cleanup_stale_old_binaries(&dst);
        assert!(!parked.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
