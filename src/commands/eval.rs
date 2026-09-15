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

    /// Run the full pipeline locally (pipeline/stages/run_all.sh) instead of Jenkins
    #[arg(long)]
    pub local: bool,

    /// Number of DeepSWE tasks (default 1)
    #[arg(long, default_value_t = 1)]
    pub n_tasks: u32,

    /// iCode delivery: binary | source (users: binary drop; developers: source)
    #[arg(long, default_value = "binary")]
    pub icode_mode: String,

    /// Official iCode *-full-* path/URL, or empty to discover under ~/.local/share/mac-k3d/
    #[arg(long)]
    pub icode_release: Option<String>,

    /// Path to iCode source tree (ICODE_MODE=source)
    #[arg(long)]
    pub icode_source: Option<String>,

    /// Eval workdir (default: ./eval-runs)
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
    if let Err(err) = crate::prepare::eval_assets::ensure_share_pipeline_reported() {
        println!("Warning: could not extract pipeline ({err}).");
    }
    let repo = discover_repo_root()?;
    let mut n_tasks = args.n_tasks.max(1);
    let mut icode_mode = normalize_mode(&args.icode_mode);
    let mut icode_release = args
        .icode_release
        .or_else(|| std::env::var("ICODE_RELEASE").ok().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(discover_icode_release);
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
        .or_else(|| std::env::var("MAC_K3D_EVAL_WORKDIR").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("eval-runs"));
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
            .with_prompt("iCode delivery")
            .items(&[
                "binary (official *-full-* or icode under ~/.local/share/mac-k3d/)",
                "source (developer git tree)",
            ])
            .default(if icode_mode == "source" { 1 } else { 0 })
            .interact()
            .map_err(|_| Error::Cancelled)?;
        icode_mode = if mode_idx == 1 {
            "source".into()
        } else {
            "binary".into()
        };
        if icode_mode == "source" {
            icode_source = Input::with_theme(&theme)
                .with_prompt("ICODE_SOURCE")
                .default(icode_source)
                .interact_text()
                .map_err(|_| Error::Cancelled)?;
        } else {
            icode_release = Input::with_theme(&theme)
                .with_prompt("ICODE_RELEASE (empty if already in ~/.local/share/mac-k3d/)")
                .default(icode_release)
                .allow_empty(true)
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

fn discover_icode_release() -> String {
    crate::prepare::eval_assets::discover_icode_release()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
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
        let root = PathBuf::from(p);
        if crate::prepare::eval_assets::looks_like_root(&root) {
            return Ok(root);
        }
    }
    let cwd = std::env::current_dir().map_err(|e| Error::Config(e.to_string()))?;
    if crate::prepare::eval_assets::looks_like_root(&cwd) {
        return Ok(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut cur = exe.parent().map(Path::to_path_buf);
        while let Some(dir) = cur {
            if crate::prepare::eval_assets::looks_like_root(&dir) {
                return Ok(dir);
            }
            cur = dir.parent().map(Path::to_path_buf);
        }
    }
    crate::prepare::eval_assets::ensure_share_pipeline()
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
        "p0" => "pipeline/stages/p0_prereqs.sh",
        "p1" => "pipeline/stages/p1_pier.sh",
        "p2" => "pipeline/stages/p2_deepswe.sh",
        "p3" => "pipeline/stages/p3_icode.sh",
        "p4" => "pipeline/stages/p4_agent.sh",
        "p5" => "pipeline/stages/p5_harness.sh",
        "p6" => "pipeline/stages/p6_baseline.sh",
        "p7" => "pipeline/stages/p7_score.sh",
        "p8" => "pipeline/stages/p8_output.sh",
        "all" => "pipeline/stages/run_all.sh",
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
    let source_q = if icode_mode == "binary" {
        ""
    } else {
        icode_source
    };
    let build_url = format!(
        "{base}/job/icode_eval/buildWithParameters?\
         HARNESS=icode&LLM=deepseek&BENCHMARK=deepswe&N_TASKS={n_tasks}\
         &ICODE_MODE={icode_mode}&ICODE_RELEASE={}&ICODE_SOURCE={}&DEEPSEEK_MODEL={}",
        urlencoding_simple(icode_release),
        urlencoding_simple(source_q),
        urlencoding_simple(model),
    );

    println!("Triggering Jenkins job icode_eval at {base} …");
    let auth = format!("{user}:{token}");
    let code = jenkins_post_http(&auth, &build_url)?;
    let code = if code == "400" {
        // Job XML rewrite can drop ParametersDefinitionProperty until the next controller config.
        let fallback = format!("{base}/job/icode_eval/build");
        jenkins_post_http(&auth, &fallback)?
    } else {
        code
    };
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

fn jenkins_post_http(auth: &str, url: &str) -> Result<String> {
    let status = Command::new("curl")
        .args([
            "-sS",
            "-u",
            auth,
            "-X",
            "POST",
            url,
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
        ])
        .output()
        .map_err(|e| Error::CommandFailed {
            cmd: "curl icode_eval trigger".into(),
            source: e.into(),
        })?;
    Ok(String::from_utf8_lossy(&status.stdout).trim().to_string())
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

    #[test]
    fn discover_icode_release_empty_without_drop() {
        let prev = std::env::var_os("MAC_K3D_SHARE");
        std::env::set_var("MAC_K3D_SHARE", "/tmp/mac-k3d-no-icode-drop-xyz");
        let got = discover_icode_release();
        match prev {
            Some(v) => std::env::set_var("MAC_K3D_SHARE", v),
            None => std::env::remove_var("MAC_K3D_SHARE"),
        }
        assert!(got.is_empty() || !got.contains("Documents/Toby/mac-k3d"));
    }

    #[test]
    fn discover_icode_release_finds_full_tarball_in_share() {
        let dir = std::env::temp_dir().join(format!(
            "mac-k3d-eval-full-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tar = dir.join("icode-linux-x86_64-full-v0.1.41.tar.gz");
        std::fs::write(&tar, b"stub").unwrap();
        let prev = std::env::var_os("MAC_K3D_SHARE");
        std::env::set_var("MAC_K3D_SHARE", &dir);
        let got = discover_icode_release();
        match prev {
            Some(v) => std::env::set_var("MAC_K3D_SHARE", v),
            None => std::env::remove_var("MAC_K3D_SHARE"),
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(got, tar.display().to_string());
    }

    #[test]
    fn looks_like_root_needs_pipeline_stages() {
        assert!(!crate::prepare::eval_assets::looks_like_root(Path::new("/tmp")));
    }
}
