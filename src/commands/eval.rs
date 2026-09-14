use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};

use crate::config::MacK3dConfig;
use crate::error::{Error, Result};

#[derive(Debug, Default, Args)]
pub struct EvalArgs {
    /// Run a single stage script (p0–p8) or omit for full flow
    #[arg(long)]
    pub stage: Option<String>,

    /// Run the full pipeline locally (scripts/eval/run_all.sh) instead of Jenkins
    #[arg(long)]
    pub local: bool,

    /// Number of DeepSWE tasks (default 1)
    #[arg(long, default_value_t = 1)]
    pub n_tasks: u32,

    /// iCode delivery: binary | source
    #[arg(long, default_value = "source")]
    pub icode_mode: String,

    /// Path or URL to icode -full- tarball / binary (ICODE_MODE=binary)
    #[arg(long)]
    pub icode_release: Option<String>,

    /// Path to iCode source tree (ICODE_MODE=source)
    #[arg(long)]
    pub icode_source: Option<String>,

    /// Eval workdir (default: ./eval-work)
    #[arg(long)]
    pub workdir: Option<PathBuf>,

    /// DeepSeek Chat Completions model id (env DEEPSEEK_MODEL)
    #[arg(long)]
    pub model: Option<String>,

    /// Skip interactive prompts (use flags / env only)
    #[arg(long)]
    pub yes: bool,
}

/// Interactive or staged iCode / DeepSeek / DeepSWE evaluation.
pub async fn run(args: EvalArgs, config: &MacK3dConfig) -> Result<()> {
    let repo = discover_repo_root()?;
    let mut n_tasks = args.n_tasks.max(1);
    let mut icode_mode = normalize_mode(&args.icode_mode);
    let mut icode_release = args
        .icode_release
        .or_else(|| std::env::var("ICODE_RELEASE").ok())
        .unwrap_or_default();
    let mut icode_source = args
        .icode_source
        .or_else(|| {
            std::env::var("ICODE_SOURCE")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or_else(|| discover_icode_source(&repo).display().to_string());
    let workdir = args
        .workdir
        .unwrap_or_else(|| PathBuf::from("eval-work"));
    let mut model = args
        .model
        .or_else(|| std::env::var("DEEPSEEK_MODEL").ok())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "deepseek-v4-pro".into());
    let local = args.local;

    if args.stage.is_none() && !args.yes && atty::is(atty::Stream::Stdin) {
        println!("mac-k3d eval — iCode harness vs DeepSeek baseline on DeepSWE\n");
        let theme = ColorfulTheme::default();
        let _h = Select::with_theme(&theme)
            .with_prompt("Harness")
            .items(&["icode"])
            .default(0)
            .interact()
            .map_err(|_| Error::Cancelled)?;
        let _l = Select::with_theme(&theme)
            .with_prompt("LLM")
            .items(&["deepseek"])
            .default(0)
            .interact()
            .map_err(|_| Error::Cancelled)?;
        let _b = Select::with_theme(&theme)
            .with_prompt("Benchmark")
            .items(&["deepswe"])
            .default(0)
            .interact()
            .map_err(|_| Error::Cancelled)?;
        n_tasks = Input::with_theme(&theme)
            .with_prompt("Number of questions (tasks)")
            .default(n_tasks)
            .interact_text()
            .map_err(|_| Error::Cancelled)?;
        let mode_idx = Select::with_theme(&theme)
            .with_prompt("iCode input")
            .items(&["source (git tree)", "binary (-full- tarball / path)"])
            .default(if icode_mode == "binary" { 1 } else { 0 })
            .interact()
            .map_err(|_| Error::Cancelled)?;
        icode_mode = if mode_idx == 1 {
            "binary".into()
        } else {
            "source".into()
        };
        if icode_mode == "source" {
            icode_source = Input::with_theme(&theme)
                .with_prompt("ICODE_SOURCE")
                .default(icode_source)
                .interact_text()
                .map_err(|_| Error::Cancelled)?;
        } else {
            icode_release = Input::with_theme(&theme)
                .with_prompt("ICODE_RELEASE (URL or path)")
                .default(icode_release)
                .interact_text()
                .map_err(|_| Error::Cancelled)?;
        }
        model = Input::with_theme(&theme)
            .with_prompt("DEEPSEEK_MODEL")
            .default(model)
            .interact_text()
            .map_err(|_| Error::Cancelled)?;
    }

    if args.stage.is_none() && !args.yes && !atty::is(atty::Stream::Stdin) {
        return Err(Error::Config(
            "not a TTY: pass --local, --stage p0..p8, or --yes (Jenkins) / --yes --local".into(),
        ));
    }

    if let Some(stage) = args.stage.as_deref() {
        return run_stage(
            &repo,
            stage,
            n_tasks,
            &icode_mode,
            &icode_release,
            &icode_source,
            &workdir,
            &model,
        );
    }

    let run_local = if local {
        true
    } else if args.yes {
        false
    } else if atty::is(atty::Stream::Stdin) {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Run locally now (--local)? No = trigger Jenkins job icode_eval")
            .default(true)
            .interact()
            .map_err(|_| Error::Cancelled)?
    } else {
        true
    };

    if run_local {
        return run_stage(
            &repo,
            "all",
            n_tasks,
            &icode_mode,
            &icode_release,
            &icode_source,
            &workdir,
            &model,
        );
    }

    trigger_jenkins_icode_eval(
        config,
        n_tasks,
        &icode_mode,
        &icode_release,
        &icode_source,
        &model,
    )
}

fn normalize_mode(s: &str) -> String {
    if s.eq_ignore_ascii_case("binary") {
        "binary".into()
    } else {
        "source".into()
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn looks_like_icode(path: &Path) -> bool {
    path.is_dir()
        && (path.join(".venv/bin/icode").is_file() || path.join("pyproject.toml").is_file())
}

fn discover_icode_source(repo: &Path) -> PathBuf {
    let home = dirs_home();
    let candidates = [
        home.join("Documents/iCode-main"),
        home.join("iCode-main"),
        home.join("src/iCode-main"),
        repo.parent().unwrap_or(repo).join("iCode-main"),
        home.join("Documents/Toby/iCode-main"),
    ];
    for c in &candidates {
        if looks_like_icode(c) {
            return c.clone();
        }
    }
    home.join("Documents/iCode-main")
}

fn discover_repo_root() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("MAC_K3D_ROOT") {
        return Ok(PathBuf::from(p));
    }
    // Prefer cwd if it contains scripts/eval
    let cwd = std::env::current_dir().map_err(|e| Error::Config(e.to_string()))?;
    if cwd.join("scripts/eval/run_all.sh").is_file() {
        return Ok(cwd);
    }
    // Walk up from executable
    if let Ok(exe) = std::env::current_exe() {
        let mut cur = exe.parent().map(Path::to_path_buf);
        while let Some(dir) = cur {
            if dir.join("scripts/eval/run_all.sh").is_file() {
                return Ok(dir);
            }
            cur = dir.parent().map(Path::to_path_buf);
        }
    }
    Err(Error::Config(
        "cannot find repo root (scripts/eval). Set MAC_K3D_ROOT or run from the mac-k3d checkout."
            .into(),
    ))
}

fn run_stage(
    repo: &Path,
    stage: &str,
    n_tasks: u32,
    icode_mode: &str,
    icode_release: &str,
    icode_source: &str,
    workdir: &Path,
    model: &str,
) -> Result<()> {
    let script = match stage {
        "p0" => "scripts/eval/p0_prereqs.sh",
        "p1" => "scripts/eval/p1_pier.sh",
        "p2" => "scripts/eval/p2_deepswe.sh",
        "p3" => "scripts/eval/p3_icode.sh",
        "p4" => "scripts/eval/p4_agent.sh",
        "p5" => "scripts/eval/p5_harness.sh",
        "p6" => "scripts/eval/p6_baseline.sh",
        "p7" => "scripts/eval/p7_score.sh",
        "p8" => "scripts/eval/p8_output.sh",
        "all" => "scripts/eval/run_all.sh",
        other => {
            return Err(Error::Config(format!(
                "unknown --stage {other}; use p0–p8 or omit for full"
            )));
        }
    };
    let path = repo.join(script);
    if !path.is_file() {
        return Err(Error::Config(format!("missing {}", path.display())));
    }

    let status = Command::new("bash")
        .arg(&path)
        .current_dir(repo)
        .env("MAC_K3D_ROOT", repo)
        .env("MAC_K3D_EVAL_WORKDIR", workdir)
        .env("N_TASKS", n_tasks.to_string())
        .env("ICODE_MODE", icode_mode)
        .env("ICODE_RELEASE", icode_release)
        .env("ICODE_SOURCE", icode_source)
        .env("HARNESS", "icode")
        .env("LLM", "deepseek")
        .env("BENCHMARK", "deepswe")
        .env("DEEPSEEK_MODEL", model)
        .env("LLM_NAME", "DeepSeek V4 Pro")
        .status()
        .map_err(|e| Error::CommandFailed {
            cmd: format!("bash {}", path.display()),
            source: e.into(),
        })?;
    if !status.success() {
        return Err(Error::CommandFailed {
            cmd: format!("bash {}", path.display()),
            source: anyhow::anyhow!("exit {:?}", status.code()),
        });
    }
    Ok(())
}

fn trigger_jenkins_icode_eval(
    config: &MacK3dConfig,
    n_tasks: u32,
    icode_mode: &str,
    icode_release: &str,
    icode_source: &str,
    model: &str,
) -> Result<()> {
    let url = config
        .jenkins_agent
        .controller_url
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            Some(format!(
                "http://localhost:{}",
                config.jenkins.host_port
            ))
        })
        .unwrap();
    let user = config
        .jenkins_agent
        .api_user
        .clone()
        .unwrap_or_else(|| "admin".into());
    let token = config
        .jenkins_agent
        .api_token
        .clone()
        .filter(|s| !s.is_empty() && s != "REPLACE_ME")
        .ok_or_else(|| {
            Error::Config(
                "Jenkins API token missing. Set jenkins_agent.api_user/api_token in config \
                 (controller machine) or use --local."
                    .into(),
            )
        })?;

    let base = url.trim_end_matches('/');
    let build_url = format!(
        "{base}/job/icode_eval/buildWithParameters?\
         HARNESS=icode&LLM=deepseek&BENCHMARK=deepswe&N_TASKS={n_tasks}\
         &ICODE_MODE={icode_mode}&ICODE_RELEASE={}&ICODE_SOURCE={}&DEEPSEEK_MODEL={}",
        urlencoding_simple(icode_release),
        urlencoding_simple(icode_source),
        urlencoding_simple(model),
    );

    println!("Triggering Jenkins job icode_eval at {base} …");
    let status = Command::new("curl")
        .args([
            "-sS",
            "-u",
            &format!("{user}:{token}"),
            "-X",
            "POST",
            &build_url,
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
        ])
        .output()
        .map_err(|e| Error::CommandFailed {
            cmd: "curl buildWithParameters icode_eval".into(),
            source: e.into(),
        })?;
    let code = String::from_utf8_lossy(&status.stdout).trim().to_string();
    if code != "201" && code != "200" && code != "302" && code != "303" {
        return Err(Error::Config(format!(
            "failed to trigger icode_eval (HTTP {code}). Ensure the job exists \
             (mac-k3d config on controller) or use --local."
        )));
    }
    println!(
        "Triggered. Watch progress in Jenkins UI:\n  {base}/job/icode_eval/\n\
         Look for PROGRESS n% lines in the console log."
    );
    let _ = config;
    Ok(())
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_icode_source_ends_with_icode_main() {
        let repo = PathBuf::from("/tmp/mac-k3d-no-such-checkout");
        let got = discover_icode_source(&repo);
        assert!(
            got.file_name().and_then(|s| s.to_str()) == Some("iCode-main"),
            "{}",
            got.display()
        );
    }
}
