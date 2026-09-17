//! OS-specific adapters for macOS and Linux.
//!
//! Shared prepare/runtime code calls these facades; compile-time `cfg` selects
//! the implementation in [`macos`] or [`linux`].

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "macos")]
use macos as os;

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    Macos,
    Linux,
}

impl HostOs {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Linux => "linux",
        }
    }
}

/// Detect the OS this binary was built for.
pub fn host_os() -> HostOs {
    #[cfg(target_os = "macos")]
    {
        HostOs::Macos
    }
    #[cfg(target_os = "linux")]
    {
        HostOs::Linux
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        // Unreachable for supported builds; kept for clarity.
        HostOs::Linux
    }
}

/// Guard for commands that require macOS or Linux.
pub fn ensure_supported_os() -> Result<()> {
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        Ok(())
    } else {
        Err(Error::UnsupportedPlatform)
    }
}

/// Backward-compatible alias used during migration.
#[deprecated(note = "use ensure_supported_os")]
pub fn ensure_macos() -> Result<()> {
    ensure_supported_os()
}

pub fn docker_display_name() -> &'static str {
    os::docker_display_name()
}

/// What to do when `docker info` has no Server after install (Linux logout, macOS Desktop).
pub fn docker_not_ready_hint() -> &'static str {
    os::docker_not_ready_hint()
}

pub fn package_manager_label() -> &'static str {
    os::package_manager_label()
}

pub fn role_prompt() -> &'static str {
    os::role_prompt()
}

pub fn machine_noun() -> &'static str {
    os::machine_noun()
}

pub fn default_agent_labels() -> Vec<String> {
    os::default_agent_labels()
}

/// systemd unit (Linux) or LaunchAgent label (macOS) for the Jenkins inbound agent.
pub fn agent_daemon_label() -> &'static str {
    os::agent_daemon_label()
}

pub fn platform_label() -> &'static str {
    host_os().as_str()
}

/// Extra mount roots to scan for storage / LoLBench (beyond `$HOME`).
pub fn scan_extra_mount_roots() -> Vec<PathBuf> {
    os::scan_extra_mount_roots()
}

pub fn docker_app_default() -> Option<PathBuf> {
    os::docker_app_default()
}

pub fn requires_docker_app() -> bool {
    os::requires_docker_app()
}

pub fn agent_path_extras() -> Vec<&'static str> {
    os::agent_path_extras()
}

pub fn preferred_shell_rc_default(home: &Path) -> PathBuf {
    os::preferred_shell_rc_default(home)
}

pub fn install_agent_daemon(
    launch_script: &Path,
    working_dir: &Path,
    path_env: &str,
) -> Result<()> {
    os::install_agent_daemon(launch_script, working_dir, path_env)
}

pub fn stop_agent_daemon() -> Result<()> {
    os::stop_agent_daemon()
}

pub fn package_install_available() -> bool {
    os::package_install_available()
}

pub fn install_package(name: &str) -> Result<()> {
    os::install_package(name)
}

pub fn harbor_bootstrap_hint() -> &'static str {
    os::harbor_bootstrap_hint()
}
