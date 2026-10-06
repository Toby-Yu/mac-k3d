//! Runs after the wizard's YAML is saved: storage directories, the installs the
//! wizard marked, the pinned Harbor, and host settings. Whatever needs root and
//! cannot be done here ends in one block for an administrator.

use std::path::Path;

use crate::config::{DependencySource, MacK3dConfig, NodeRole};
use crate::error::{Error, Result};
use crate::platform::{self, RootStep};
use crate::prepare::{install, resources, root_steps};

/// Install order. Harbor last: it only needs uv, never root.
const INSTALL_ORDER: [&str; 7] = ["docker", "git", "java", "k3d", "kubectl", "helm", "harbor"];

/// Bring this host in line with `config`. Updates the dependency entries in
/// place, so the caller saves the config again afterwards.
pub fn apply(config: &mut MacK3dConfig, config_path: &Path) -> Result<()> {
    ensure_storage_dirs(config)?;
    let mut sudo = Sudo::default();
    let mut for_root = install_pending(config, &mut sudo)?;

    if matches!(config.role, NodeRole::Worker) {
        let host = platform::worker_host_root_steps();
        if !host.is_empty() {
            if sudo.allowed() {
                root_steps::run(&host)?;
                if host.iter().any(|s| s.command.contains("usermod")) {
                    println!("Log out and back in so the docker group applies to new shells.");
                }
            } else {
                for_root.extend(host);
            }
        }
    }
    if !for_root.is_empty() {
        return Err(Error::NeedsRoot(root_steps::render(&for_root, config_path)));
    }

    if matches!(config.role, NodeRole::Controller) {
        controller_notes(config);
    }
    let disk_path = config.storage.base_dir.as_deref().unwrap_or(Path::new("/"));
    resources::ensure_disk_min(disk_path, config.disk_min_gb())?;
    resources::ensure_ram_min(8)?;
    Ok(())
}

/// Asks for sudo at most once per run, and only when something needs it.
#[derive(Default)]
struct Sudo(Option<bool>);

impl Sudo {
    fn allowed(&mut self) -> bool {
        *self.0.get_or_insert_with(platform::can_elevate)
    }
}

/// Install what the wizard marked. A tool that appeared since the wizard ran
/// (root installed it) is recorded instead. Returns the root steps left over.
fn install_pending(config: &mut MacK3dConfig, sudo: &mut Sudo) -> Result<Vec<RootStep>> {
    let mut for_root = Vec::new();
    for name in INSTALL_ORDER {
        let Some(entry) = config.dependencies.entry_mut(name) else {
            continue;
        };
        if entry.source != DependencySource::Install {
            continue;
        }
        if name != "harbor" {
            if let Some(tool) = install::discover_single(name) {
                println!("{name}: found {}; nothing to install", tool.binary.display());
                *entry = install::tool_to_entry(&tool);
                continue;
            }
            if platform::root_install_command(name).is_some() && !sudo.allowed() {
                for_root.extend(root_steps::for_packages(&[name]));
                continue;
            }
        }
        *entry = install::install_and_discover(name)?;
    }
    Ok(for_root)
}

fn ensure_storage_dirs(config: &MacK3dConfig) -> Result<()> {
    let storage = &config.storage;
    let dirs = [
        storage.base_dir.clone(),
        storage.docker_dir(),
        storage.k3d_dir(),
        storage.jenkins_dir(),
        storage.downloads_dir(),
    ];
    for dir in dirs.into_iter().flatten() {
        let path = dir.display().to_string();
        std::fs::create_dir_all(&dir)
            .map_err(|e| Error::Config(format!("failed to create {path}: {e}")))?;
        tracing::info!(%path, "created storage directory");
    }
    Ok(())
}

fn controller_notes(config: &MacK3dConfig) {
    println!(
        "\nController: after `mac-k3d start`, ensure Lockable Resources label '{}' exists.",
        config.resources.cpu_cores_label
    );
    let _ = resources::ensure_cpu_cores_label_on_controller(
        &format!("http://localhost:{}", config.jenkins.host_port),
        &config.resources.cpu_cores_label,
    );
    if let Some(docker_dir) = config.storage.docker_dir() {
        println!(
            "Recommended Docker data path: {} (configure {} data-root if desired).",
            docker_dir.display(),
            platform::docker_display_name()
        );
    }
}
