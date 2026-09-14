use std::path::PathBuf;
use std::process::Command;

use crate::config::{DependencyEntry, DependencySource};
use crate::error::{Error, Result};
use crate::platform;
use crate::prepare::discovery::{self, DiscoveredTool};
use crate::prepare::path_env;

/// Install a dependency and return the discovered entry afterward.
pub fn install_and_discover(name: &str) -> Result<DependencyEntry> {
    match name {
        "harbor" => install_harbor(),
        "java" | "docker" | "k3d" | "kubectl" | "helm" | "git" | "uv" => {
            platform::install_package(name)?;
            discover_single(name)
                .map(|t| tool_to_entry(&t))
                .ok_or_else(|| {
                    Error::DependencyMissing(format!(
                        "{name} installed but binary not found on PATH"
                    ))
                })
        }
        other => Err(Error::Config(format!(
            "unknown dependency for install: {other}"
        ))),
    }
}

/// git + uv + Pier 0.3.x for worker eval. Safe to re-run.
pub fn ensure_eval_toolchain() -> Result<()> {
    path_env::ensure_user_local_bin_in_process()?;
    if discovery::which("git").is_none() {
        platform::install_package("git")?;
    }
    if discovery::which("uv").is_none() {
        platform::install_package("uv")?;
        path_env::ensure_user_local_bin_in_process()?;
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let _ = path_env::prepend_to_process_path(&home.join(".cargo").join("bin"));
            let _ = path_env::prepend_to_process_path(&home.join(".local").join("bin"));
        }
    }
    path_env::ensure_user_local_bin(true)?;
    if discovery::which("git").is_none() {
        return Err(Error::DependencyMissing(
            "git not on PATH after install".into(),
        ));
    }
    if discovery::which("uv").is_none() {
        return Err(Error::DependencyMissing(
            "uv not on PATH after install".into(),
        ));
    }
    if discovery::which("pier").is_none() {
        tracing::info!("installing datacurve-pier via uv tool");
        if let Err(e) = run_cmd("uv", &["tool", "install", "datacurve-pier"]) {
            tracing::warn!(error = %e, "uv tool install datacurve-pier failed; trying git URL");
            run_cmd(
                "uv",
                &["tool", "install", "git+https://github.com/datacurve-ai/pier"],
            )?;
        }
        path_env::ensure_user_local_bin_in_process()?;
    }
    let pier = discovery::which("pier").ok_or_else(|| {
        Error::DependencyMissing("pier not on PATH after uv tool install".into())
    })?;
    let help = Command::new(&pier)
        .args(["run", "--help"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr))
        .unwrap_or_default();
    if !help.contains("--agent-import-path") {
        return Err(Error::DependencyMissing(
            "this pier has no --agent-import-path; need datacurve-pier 0.3.x (uv tool install datacurve-pier)"
                .into(),
        ));
    }
    Ok(())
}

fn install_harbor() -> Result<DependencyEntry> {
    path_env::ensure_user_local_bin_in_process()?;

    if discovery::which("uv").is_some() {
        tracing::info!("installing harbor via uv tool install");
        run_cmd("uv", &["tool", "install", "harbor"])?;
    } else if discovery::which("pipx").is_some() {
        tracing::info!("installing harbor via pipx");
        run_cmd("pipx", &["install", "harbor"])?;
    } else if platform::package_install_available() {
        tracing::info!("bootstrapping uv, then harbor");
        platform::install_package("uv")?;
        path_env::ensure_user_local_bin_in_process()?;
        // uv may land in ~/.local/bin or ~/.cargo/bin after curl install.
        if discovery::which("uv").is_none() {
            if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
                let _ = path_env::prepend_to_process_path(&home.join(".cargo").join("bin"));
                let _ = path_env::prepend_to_process_path(&home.join(".local").join("bin"));
            }
        }
        run_cmd("uv", &["tool", "install", "harbor"])?;
    } else {
        return Err(Error::DependencyMissing(format!(
            "harbor: need uv, pipx, or {} to install",
            platform::package_manager_label()
        )));
    }

    path_env::ensure_user_local_bin(true)?;

    discover_harbor()
        .map(|t| tool_to_entry(&t))
        .ok_or_else(|| {
            Error::DependencyMissing(
                "harbor installed but binary not found (checked PATH and ~/.local/bin)".into(),
            )
        })
}

fn discover_harbor() -> Option<DiscoveredTool> {
    discovery::discover_all().harbor.or_else(|| {
        let bin = path_env::user_local_bin()?.join("harbor");
        bin.exists().then(|| DiscoveredTool {
            binary: bin,
            app: None,
            version_hint: None,
        })
    })
}

fn run_cmd(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program).args(args).status().map_err(|e| {
        Error::CommandFailed {
            cmd: format!("{program} {}", args.join(" ")),
            source: e.into(),
        }
    })?;
    if !status.success() {
        return Err(Error::CommandFailed {
            cmd: format!("{program} {}", args.join(" ")),
            source: anyhow::anyhow!("exit code {:?}", status.code()),
        });
    }
    Ok(())
}

pub fn brew_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        discovery::which("brew").is_some()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn discover_single(name: &str) -> Option<DiscoveredTool> {
    let deps = discovery::discover_all();
    match name {
        "docker" => deps.docker,
        "k3d" => deps.k3d,
        "kubectl" => deps.kubectl,
        "helm" => deps.helm,
        "harbor" => deps.harbor,
        "java" => deps.java,
        _ => None,
    }
}

pub fn tool_to_entry(tool: &DiscoveredTool) -> DependencyEntry {
    DependencyEntry {
        source: DependencySource::Existing,
        binary: Some(tool.binary.clone()),
        app: tool.app.clone(),
    }
}

pub fn entry_from_path(path: PathBuf, app: Option<PathBuf>) -> Result<DependencyEntry> {
    if !path.exists() {
        return Err(Error::Validation(format!(
            "binary not found: {}",
            path.display()
        )));
    }
    Ok(DependencyEntry {
        source: DependencySource::Existing,
        binary: Some(path),
        app,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_manager_probe() {
        let _ = platform::package_install_available();
        let _ = brew_available();
    }
}
