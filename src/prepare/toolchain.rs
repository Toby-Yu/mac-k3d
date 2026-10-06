//! The eval toolchain a worker needs on its own account: uv, and Harbor at the
//! version the pipeline asserts. Both sides read the pin from
//! `pipeline/config/toolchain.env`, so `setup` and the env phase cannot disagree.

use std::path::PathBuf;
use std::process::Command;

use crate::error::{Error, Result};
use crate::platform;
use crate::prepare::{discovery, path_env};

const TOOLCHAIN_ENV: &str = include_str!("../../pipeline/config/toolchain.env");

/// Value of `KEY=value` in toolchain.env.
pub fn pin(key: &str) -> Option<&'static str> {
    parse_pin(TOOLCHAIN_ENV, key)
}

fn parse_pin<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

/// Harbor version the pipeline's env phase checks.
pub fn harbor_version() -> &'static str {
    pin("HARBOR_VERSION").expect("pipeline/config/toolchain.env must set HARBOR_VERSION")
}

/// Harbor binary on PATH or in `~/.local/bin`.
pub fn harbor_binary() -> Option<PathBuf> {
    discovery::which("harbor").or_else(|| {
        path_env::user_local_bin()
            .map(|d| d.join("harbor"))
            .filter(|p| p.exists())
    })
}

/// `harbor --version` of the installed binary, if any.
pub fn installed_harbor_version() -> Option<String> {
    let bin = harbor_binary()?;
    let out = Command::new(&bin).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

/// Last word of the first line: `0.22.0` and `harbor 0.22.0` both give `0.22.0`.
fn parse_version(text: &str) -> Option<String> {
    text.lines()
        .next()?
        .split_whitespace()
        .last()
        .map(str::to_string)
}

/// Install or replace Harbor so `harbor --version` equals the pin. User-level
/// only (`uv tool install` into `~/.local/bin`); never needs root.
pub fn ensure_harbor() -> Result<PathBuf> {
    path_env::ensure_user_local_bin_in_process()?;
    let want = harbor_version();
    match installed_harbor_version() {
        Some(got) if got == want => {
            println!("harbor {want}: already installed");
        }
        got => {
            ensure_uv()?;
            match got {
                Some(other) => println!("harbor {other} found; installing the pinned {want}"),
                None => println!("Installing harbor {want} with uv"),
            }
            run("uv", &["tool", "install", "--force", &format!("harbor=={want}")])?;
            path_env::ensure_user_local_bin(true)?;
            let now = installed_harbor_version();
            if now.as_deref() != Some(want) {
                return Err(Error::DependencyMissing(format!(
                    "harbor --version is {}, wanted {want} after uv tool install",
                    now.as_deref().unwrap_or("missing")
                )));
            }
        }
    }
    harbor_binary().ok_or_else(|| Error::DependencyMissing("harbor not on PATH after install".into()))
}

/// uv in `~/.local/bin`, installed with its own script (no root).
pub fn ensure_uv() -> Result<()> {
    path_env::ensure_user_local_bin_in_process()?;
    if discovery::which("uv").is_some() {
        return Ok(());
    }
    platform::install_package("uv")?;
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let _ = path_env::prepend_to_process_path(&home.join(".local").join("bin"));
        let _ = path_env::prepend_to_process_path(&home.join(".cargo").join("bin"));
    }
    discovery::which("uv")
        .map(|_| ())
        .ok_or_else(|| Error::DependencyMissing("uv not on PATH after install".into()))
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| Error::CommandFailed {
            cmd: format!("{program} {}", args.join(" ")),
            source: e.into(),
        })?;
    if !status.success() {
        return Err(Error::CommandFailed {
            cmd: format!("{program} {}", args.join(" ")),
            source: anyhow::anyhow!("exit code {:?}", status.code()),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_reads_key_value_lines_only() {
        let text = "# HARBOR_VERSION=9.9.9\nHARBOR_VERSION=0.22.0\nEMPTY=\n";
        assert_eq!(parse_pin(text, "HARBOR_VERSION"), Some("0.22.0"));
        assert_eq!(parse_pin(text, "EMPTY"), None);
        assert_eq!(parse_pin(text, "MISSING"), None);
    }

    #[test]
    fn harbor_pin_matches_what_common_sh_reads() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let file = std::fs::read_to_string(root.join("pipeline/config/toolchain.env")).unwrap();
        assert_eq!(parse_pin(&file, "HARBOR_VERSION"), Some(harbor_version()));
        let common = std::fs::read_to_string(root.join("pipeline/stages/_common.sh")).unwrap();
        assert!(common.contains("pipeline/config/toolchain.env"));
        assert!(
            !common.contains("HARBOR_VERSION:-"),
            "_common.sh must take the Harbor pin from toolchain.env, not a literal"
        );
    }

    #[test]
    fn version_is_last_word_of_first_line() {
        assert_eq!(parse_version("0.22.0\n").as_deref(), Some("0.22.0"));
        assert_eq!(parse_version("harbor 0.24.0\nmore").as_deref(), Some("0.24.0"));
        assert_eq!(parse_version(""), None);
    }
}
