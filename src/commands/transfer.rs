use std::path::{Path, PathBuf};

use clap::Args;

use crate::config::{MacK3dConfig, NodeRole};
use crate::error::{Error, Result};

const EXPORT_SAME_PATH_MSG: &str = "export -o would overwrite the source config; pick another path";

#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Destination YAML (sanitized; no API tokens or host-local paths)
    #[arg(short, long)]
    pub output: PathBuf,
}

#[derive(Debug, Args)]
pub struct ImportArgs {
    /// Sanitized YAML copied from another machine
    pub source: PathBuf,

    /// Overwrite the destination if it already exists
    #[arg(long)]
    pub force: bool,
}

/// Copy a live config to a shareable YAML. Does not read `credentials.pending.yaml`.
pub fn run_export(args: ExportArgs, config_path: Option<&Path>) -> Result<()> {
    let source = MacK3dConfig::resolve_config_path(config_path);
    let config = MacK3dConfig::load_file(&source)?;
    if paths_equal(&source, &args.output) {
        return Err(Error::Config(EXPORT_SAME_PATH_MSG.into()));
    }
    config.save_sanitized_export(&args.output)?;
    println!(
        "Exported sanitized {} config to {}.\n\
         Secrets and host paths were stripped. Edit jenkins_job / labels if this lab differs, then:\n\
           mac-k3d import {}",
        role_name(config.role),
        args.output.display(),
        args.output.display()
    );
    Ok(())
}

/// Write a sanitized YAML onto this machine. Does not start Docker, k3d, or Jenkins.
pub fn run_import(args: ImportArgs, dest_override: Option<&Path>) -> Result<()> {
    let incoming = MacK3dConfig::load_file(&args.source)?;
    let dest = dest_override
        .map(PathBuf::from)
        .unwrap_or_else(|| MacK3dConfig::default_path_for_role(incoming.role));
    write_imported_config(incoming, &dest, args.force)?;
    Ok(())
}

fn write_imported_config(incoming: MacK3dConfig, dest: &Path, force: bool) -> Result<MacK3dConfig> {
    if dest.exists() && !force {
        return Err(Error::Config(format!(
            "{} already exists; pass --force to overwrite",
            dest.display()
        )));
    }
    let mut cfg = incoming.for_export();
    cfg.apply_host_platform();
    cfg.save(Some(dest))?;
    print_import_next_steps(&cfg, dest);
    Ok(cfg)
}

fn print_import_next_steps(config: &MacK3dConfig, dest: &Path) {
    let c = dest.display();
    println!(
        "Imported {} config to {} (write only; Docker/k3d/Jenkins were not started).",
        role_name(config.role),
        c
    );
    match config.role {
        NodeRole::Worker => {
            let token_ok = config
                .jenkins_agent
                .api_token
                .as_ref()
                .map(|s| !s.trim().is_empty() && s != "REPLACE_ME")
                .unwrap_or(false);
            if !token_ok {
                println!(
                    "Put jenkins_agent.api_user / api_token in {c} then:\n  mac-k3d config -c {c}"
                );
            } else {
                println!("Next (worker — do not run start on this file):\n  mac-k3d config -c {c}");
            }
        }
        NodeRole::Controller => {
            println!(
                "Next (controller):\n  mac-k3d setup -c {c}\n\
                 Or if Docker/k3d already exist:\n  mac-k3d start -c {c}\n  mac-k3d config -c {c} --skip-secrets"
            );
        }
        NodeRole::Standalone => {
            println!("Next:\n  mac-k3d setup -c {c}");
        }
    }
}

fn role_name(role: NodeRole) -> &'static str {
    match role {
        NodeRole::Standalone => "standalone",
        NodeRole::Controller => "controller",
        NodeRole::Worker => "worker",
    }
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(aa), Ok(bb)) => aa == bb,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::JenkinsJobConfig;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mac-k3d-xfer-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn worker_with_token() -> MacK3dConfig {
        let mut cfg = MacK3dConfig::default();
        cfg.role = NodeRole::Worker;
        cfg.jenkins_agent.api_token = Some("test-jenkins-token".into());
        cfg.jenkins_job = JenkinsJobConfig {
            default_task: "ruff_1".into(),
            ..JenkinsJobConfig::default()
        };
        cfg
    }

    #[test]
    fn import_infers_worker_dest_when_no_override() {
        let incoming = worker_with_token();
        assert_eq!(
            MacK3dConfig::default_path_for_role(incoming.role),
            MacK3dConfig::default_worker_path()
        );
    }

    #[test]
    fn import_refuses_overwrite_without_force() {
        let dir = temp_dir("ow");
        let dest = dir.join("worker.yaml");
        std::fs::write(&dest, "role: worker\n").unwrap();
        let err = write_imported_config(worker_with_token(), &dest, false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("already exists"), "{msg}");
        assert!(msg.contains("--force"), "{msg}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_force_overwrites_and_strips_token() {
        let dir = temp_dir("force");
        let dest = dir.join("worker.yaml");
        std::fs::write(&dest, "role: worker\napi_token: leftover\n").unwrap();
        let written = write_imported_config(worker_with_token(), &dest, true).unwrap();
        assert!(written.jenkins_agent.api_token.is_none());
        assert_eq!(written.jenkins_job.default_task, "ruff_1");
        assert_eq!(
            written.platform.as_deref(),
            Some(crate::platform::host_os().as_str())
        );
        let text = std::fs::read_to_string(&dest).unwrap();
        assert!(!text.contains("test-jenkins-token"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_roundtrip_preserves_task_not_token() {
        let dir = temp_dir("rt");
        let src = dir.join("live.yaml");
        let out = dir.join("portable.yaml");
        worker_with_token().save(Some(&src)).unwrap();
        run_export(
            ExportArgs {
                output: out.clone(),
            },
            Some(&src),
        )
        .unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(!text.contains("test-jenkins-token"));
        let imported = MacK3dConfig::load_file(&out).unwrap();
        assert_eq!(imported.jenkins_job.default_task, "ruff_1");
        assert!(imported.jenkins_agent.api_token.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_refuses_same_path() {
        let dir = temp_dir("same");
        let src = dir.join("live.yaml");
        worker_with_token().save(Some(&src)).unwrap();
        let err = run_export(
            ExportArgs {
                output: src.clone(),
            },
            Some(&src),
        )
        .unwrap_err();
        assert!(err.to_string().contains("overwrite the source config"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
