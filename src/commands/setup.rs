use std::path::{Path, PathBuf};

use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::config::{AgentLauncher, MacK3dConfig, NodeRole};
use crate::error::{Error, Result};
use crate::platform::ensure_supported_os;
use crate::runtime::jenkins;

use super::{ConfigArgs, PrepareArgs, StartArgs};

#[derive(Debug, Default, Args)]
pub struct SetupArgs {
    /// Override minimum free disk (GB) for prepare validation (0 = role default)
    #[arg(long)]
    pub disk_min_gb: Option<u64>,

    /// Apply an existing worker YAML and register (or, with launcher: existing, update) its agent, with no prompts
    #[arg(long)]
    pub non_interactive: bool,
}

/// Interactive first-run: wizard, then start (controller/standalone) or config (worker).
pub async fn run(
    args: SetupArgs,
    _config: &MacK3dConfig,
    config_path: Option<&Path>,
) -> Result<()> {
    ensure_supported_os()?;

    let config_path: PathBuf = config_path
        .map(PathBuf::from)
        .unwrap_or_else(MacK3dConfig::default_config_path);

    if args.non_interactive {
        return run_non_interactive(args.disk_min_gb, &config_path).await;
    }

    println!("mac-k3d setup — prepare this computer as a controller or worker.\n");

    super::run_prepare(
        PrepareArgs {
            interactive: false,
            non_interactive: false,
            init_config: false,
            disk_min_gb: args.disk_min_gb,
        },
        &MacK3dConfig::load(Some(&config_path))?,
        Some(&config_path),
    )
    .await?;

    let config = MacK3dConfig::load(Some(&config_path))?;

    if !Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Continue and apply now (start/config)?")
        .default(true)
        .interact()
        .map_err(|_| crate::error::Error::Cancelled)?
    {
        print_manual_next_steps(&config, &config_path);
        return Ok(());
    }

    match config.role {
        NodeRole::Worker => {
            println!("\nWorker: registering Jenkins agent (not starting k3d/Jenkins)…\n");
            super::run_config(ConfigArgs::default(), &config, Some(&config_path)).await?;
        }
        NodeRole::Controller | NodeRole::Standalone => {
            println!("\nStarting Docker / k3d / Jenkins (if enabled)…\n");
            super::run_start(StartArgs::default(), &config, Some(&config_path)).await?;
            println!("\nApplying kubeconfig / Jenkins job / credentials…\n");
            super::run_config(ConfigArgs::default(), &config, Some(&config_path)).await?;
        }
    }

    print_done_summary(&config, &config_path);
    Ok(())
}

/// Same installs and agent registration as the wizard path, from a YAML written
/// beforehand (for example by a provisioning job). No TTY is read.
async fn run_non_interactive(disk_min_gb: Option<u64>, config_path: &Path) -> Result<()> {
    let mut config = load_worker_config(config_path)?;
    if let Some(min) = disk_min_gb {
        config.resources.disk_min_gb = min;
    }
    println!(
        "mac-k3d setup --non-interactive — applying worker config {}\n",
        config_path.display()
    );
    super::prepare::apply_and_validate(config, config_path)?;

    let config = MacK3dConfig::load_file(config_path)?;
    match config.jenkins_agent.launcher {
        AgentLauncher::Inbound => {
            println!("\nWorker: registering Jenkins agent (not starting k3d/Jenkins)…\n")
        }
        AgentLauncher::Existing => println!("\nWorker: updating existing Jenkins node…\n"),
    }
    super::run_config(ConfigArgs::default(), &config, Some(config_path)).await?;

    print_done_summary(&config, config_path);
    Ok(())
}

fn load_worker_config(config_path: &Path) -> Result<MacK3dConfig> {
    if !config_path.exists() {
        return Err(Error::Config(format!(
            "setup --non-interactive needs an existing worker YAML; no config at {}",
            config_path.display()
        )));
    }
    let config = MacK3dConfig::load_file(config_path)?;
    if config.role != NodeRole::Worker {
        return Err(Error::Config(format!(
            "setup --non-interactive only applies role: worker; {} is not a worker config. Run `mac-k3d setup -c {}` in a terminal",
            config_path.display(),
            config_path.display()
        )));
    }
    Ok(config)
}

fn print_manual_next_steps(config: &MacK3dConfig, config_path: &Path) {
    let c = config_path.display();
    println!("\nConfig written to {c}.");
    match config.role {
        NodeRole::Worker => {
            println!("Next (worker — do not run start on this file):");
            println!("  mac-k3d config -c {c}");
        }
        NodeRole::Controller => {
            println!("Next (controller):");
            println!("  mac-k3d start -c {c}");
            println!("  mac-k3d config -c {c} --show-jenkins");
        }
        NodeRole::Standalone => {
            println!("Next:");
            println!("  mac-k3d start -c {c}");
            println!("  mac-k3d config -c {c}");
        }
    }
}

fn print_done_summary(config: &MacK3dConfig, config_path: &Path) {
    let role = match config.role {
        NodeRole::Standalone => "standalone",
        NodeRole::Controller => "controller",
        NodeRole::Worker => "worker",
    };
    let jenkins = if config.jenkins.enabled {
        jenkins::ui_url(config)
    } else {
        config
            .jenkins_agent
            .controller_url
            .clone()
            .unwrap_or_else(|| "(none)".into())
    };
    let agent = match config.role {
        NodeRole::Worker if config.jenkins_agent.launcher == AgentLauncher::Existing => format!(
            "node {} (its own launcher)",
            config.jenkins_agent.name.as_deref().unwrap_or("?")
        ),
        NodeRole::Worker => crate::platform::agent_daemon_label().to_string(),
        _ => "(n/a — not a Jenkins agent role)".into(),
    };

    println!(
        "\n=== mac-k3d setup complete ===\n\
           Role:        {role}\n\
           Config:      {}\n\
           Jenkins UI:  {jenkins}\n\
           Agent:       {agent}\n",
        config_path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_interactive_refuses_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("worker.yaml");
        let err = load_worker_config(&path).unwrap_err().to_string();
        assert!(err.contains("needs an existing worker YAML"), "{err}");
        assert!(err.contains(&path.display().to_string()), "{err}");
    }

    #[test]
    fn non_interactive_refuses_a_controller_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "role: controller\n").unwrap();
        let err = load_worker_config(&path).unwrap_err().to_string();
        assert!(err.contains("only applies role: worker"), "{err}");
    }

    #[test]
    fn non_interactive_loads_a_worker_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("worker.yaml");
        std::fs::write(
            &path,
            "role: worker\njenkins_agent:\n  name: pomegranate-1\n  labels: [linux, docker, lolbench]\n  cpu_cores: 16\n",
        )
        .unwrap();
        let config = load_worker_config(&path).unwrap();
        assert_eq!(config.role, NodeRole::Worker);
        assert_eq!(config.jenkins_agent.name.as_deref(), Some("pomegranate-1"));
        assert_eq!(config.jenkins_agent.cpu_cores, 16);
        assert_eq!(config.jenkins_agent.launcher, AgentLauncher::Inbound);
    }

    #[test]
    fn non_interactive_loads_an_existing_launcher_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("worker.yaml");
        std::fs::write(
            &path,
            "role: worker\njenkins_agent:\n  launcher: existing\n  name: pomegranate-1\n  cpu_cores: 16\n",
        )
        .unwrap();
        let config = load_worker_config(&path).unwrap();
        assert_eq!(config.jenkins_agent.launcher, AgentLauncher::Existing);
    }
}
