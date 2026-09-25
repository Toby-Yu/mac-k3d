use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Password, Select};

use crate::config::MacK3dConfig;
use crate::error::{Error, Result};
use crate::eval_catalog;

#[derive(Debug, Default, Args)]
pub struct EvalArgs {
    /// Run a single stage script (p0–p8) or omit for full flow
    #[arg(long)]
    pub stage: Option<String>,

    /// Run the full pipeline locally (pipeline/stages/run_all.sh) instead of Jenkins
    #[arg(long)]
    pub local: bool,

    /// Number of tasks when TASK is empty (default 1; *_one_task jobs stay at 1)
    #[arg(long, default_value_t = 1)]
    pub n_tasks: u32,

    /// Benchmark: deepswe | lolbench | swebenchpro
    #[arg(long)]
    pub benchmark: Option<String>,

    /// One question id (DeepSWE/SWE-bench Pro task dir or LoLBench harbor_tasks id)
    #[arg(long)]
    pub task: Option<String>,

    /// iCode delivery: release | git (`binary` is an alias of release)
    #[arg(long, default_value = "release")]
    pub icode_mode: String,

    /// Official iCode *-full-* path/URL for `--local` / `--stage` (empty = persist then ~/.local/share/mac-k3d/).
    /// Jenkins release mode uses UI upload `ICODE_RELEASE_FILE`, not this flag.
    #[arg(long)]
    pub icode_release: Option<String>,

    /// Git URL for ICODE_MODE=git (https, allow-listed host)
    #[arg(long)]
    pub icode_git_url: Option<String>,

    /// Git tag, commit SHA, branch, or pull-request number (ICODE_MODE=git; empty = main)
    #[arg(long)]
    pub icode_git_ref: Option<String>,

    /// How to check out ICODE_GIT_REF: branch | tag | commit | pr
    #[arg(long)]
    pub icode_git_ref_kind: Option<String>,

    /// Eval workdir (default: ./eval-runs)
    #[arg(long)]
    pub workdir: Option<PathBuf>,

    /// DeepSeek Chat Completions model id (catalog; env DEEPSEEK_MODEL)
    #[arg(long)]
    pub model: Option<String>,

    /// Skip interactive prompts (use flags / env only)
    #[arg(long)]
    pub yes: bool,

    /// Extract embedded pipeline/ into ~/.local/share/mac-k3d and exit (Jenkins Prepare).
    #[arg(long)]
    pub sync_pipeline: bool,
}

/// Interactive or staged iCode / DeepSeek / DeepSWE evaluation.
pub async fn run(args: EvalArgs, config: &MacK3dConfig) -> Result<()> {
    if args.sync_pipeline {
        let share = crate::prepare::eval_assets::ensure_share_pipeline()?;
        println!("Extracted pipeline to {}", share.join("pipeline").display());
        return Ok(());
    }
    if let Err(err) = crate::prepare::eval_assets::ensure_share_pipeline_reported() {
        println!("Warning: could not extract pipeline ({err}).");
    }
    let repo = discover_repo_root()?;
    let mut n_tasks = args.n_tasks.max(1);
    let env_bench = std::env::var("BENCHMARK").ok();
    let mut benchmark = normalize_benchmark(
        args.benchmark
            .as_deref()
            .or(env_bench.as_deref())
            .unwrap_or("deepswe"),
    )?;
    let mut task = inherit_eval_task(
        args.task.clone(),
        args.benchmark.as_deref(),
        std::env::var("TASK").ok(),
    );
    let mut icode_mode = eval_catalog::normalize_icode_mode(&args.icode_mode)?;
    let stored_paths = crate::prepare::icode_paths::load();
    let mut icode_release = args
        .icode_release
        .or_else(|| {
            std::env::var("ICODE_RELEASE")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| {
            let s = stored_paths.release.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        })
        .unwrap_or_else(discover_icode_release);
    let mut icode_git_url = args
        .icode_git_url
        .or_else(|| {
            std::env::var("ICODE_GIT_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| {
            let d = config.jenkins_job.default_icode_git_url.trim();
            if d.is_empty() {
                None
            } else {
                Some(d.to_string())
            }
        })
        .unwrap_or_default();
    let mut icode_git_ref = args
        .icode_git_ref
        .or_else(|| {
            std::env::var("ICODE_GIT_REF")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| {
            let d = config.jenkins_job.default_icode_git_ref.trim();
            if d.is_empty() {
                None
            } else {
                Some(d.to_string())
            }
        })
        .unwrap_or_else(|| "main".into());
    let mut icode_git_ref_kind = args
        .icode_git_ref_kind
        .or_else(|| {
            std::env::var("ICODE_GIT_REF_KIND")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or_else(|| "branch".into());
    let workdir = args
        .workdir
        .or_else(|| {
            std::env::var("MAC_K3D_EVAL_WORKDIR")
                .ok()
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from("eval-runs"));
    let mut model = args
        .model
        .or_else(|| std::env::var("DEEPSEEK_MODEL").ok())
        .or_else(|| {
            let d = config.jenkins_job.default_deepseek_model.trim();
            if d.is_empty() {
                None
            } else {
                Some(d.to_string())
            }
        })
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| eval_catalog::default_model().into());
    let local = args.local;

    if args.stage.is_none() && !args.yes && atty::is(atty::Stream::Stdin) {
        println!("mac-k3d eval — iCode harness vs DeepSeek baseline (DeepSWE or LoLBench)\n");
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
        let benches: Vec<&str> = eval_catalog::BENCHMARKS.to_vec();
        let default_b = benches
            .iter()
            .position(|b| *b == benchmark.as_str())
            .unwrap_or(0);
        let b_idx = Select::with_theme(&theme)
            .with_prompt("Benchmark")
            .items(&benches)
            .default(default_b)
            .interact()
            .map_err(|_| Error::Cancelled)?;
        benchmark = benches[b_idx].to_string();
        if benchmark == "lolbench" && task.is_empty() {
            task = "ruff_1".into();
        }
        task = Input::with_theme(&theme)
            .with_prompt("TASK (one question id; empty DeepSWE/SWE-bench Pro = first alphabetical)")
            .default(task)
            .allow_empty(true)
            .interact_text()
            .map_err(|_| Error::Cancelled)?;
        n_tasks = Input::with_theme(&theme)
            .with_prompt("Number of questions (used only when TASK is empty)")
            .default(n_tasks)
            .interact_text()
            .map_err(|_| Error::Cancelled)?;
        let mode_idx = Select::with_theme(&theme)
            .with_prompt("iCode delivery")
            .items(&[
                "release (official *-full-* or icode under ~/.local/share/mac-k3d/)",
                "git (clone allow-listed URL at tag / commit / branch / pull request)",
            ])
            .default(if icode_mode == "git" { 1 } else { 0 })
            .interact()
            .map_err(|_| Error::Cancelled)?;
        icode_mode = if mode_idx == 1 {
            "git".into()
        } else {
            "release".into()
        };
        if icode_mode == "git" {
            icode_git_url = Input::with_theme(&theme)
                .with_prompt("ICODE_GIT_URL (https://github.com/… or gitcode.com)")
                .default(icode_git_url)
                .interact_text()
                .map_err(|_| Error::Cancelled)?;
            let kind_items = [
                "branch (checkout the tip of this branch)",
                "tag (release or lightweight tag)",
                "commit (SHA, including a pull-request commit to score before merge)",
                "pr (pull-request number; checks out that PR tip)",
            ];
            let kind_default = match icode_git_ref_kind.as_str() {
                "tag" => 1,
                "commit" => 2,
                "pr" => 3,
                "auto" if eval_catalog::looks_like_git_commit(&icode_git_ref) => 2,
                _ => 0,
            };
            let kind_idx = Select::with_theme(&theme)
                .with_prompt("ICODE_GIT_REF kind")
                .items(&kind_items)
                .default(kind_default)
                .interact()
                .map_err(|_| Error::Cancelled)?;
            icode_git_ref_kind = match kind_idx {
                1 => "tag".into(),
                2 => "commit".into(),
                3 => "pr".into(),
                _ => "branch".into(),
            };
            let ref_prompt = match icode_git_ref_kind.as_str() {
                "tag" => "ICODE_GIT_REF (tag name)",
                "commit" => {
                    "ICODE_GIT_REF (commit SHA; use the PR commit you want to evaluate, 7–40 hex)"
                }
                "pr" => "ICODE_GIT_REF (pull-request number)",
                _ => "ICODE_GIT_REF (branch name; tip is checked out)",
            };
            icode_git_ref = Input::with_theme(&theme)
                .with_prompt(ref_prompt)
                .default(icode_git_ref)
                .interact_text()
                .map_err(|_| Error::Cancelled)?;
        } else {
            icode_release = Input::with_theme(&theme)
                .with_prompt(
                    "ICODE_RELEASE (local path; empty = persist file or ~/.local/share/mac-k3d/)",
                )
                .default(icode_release)
                .allow_empty(true)
                .interact_text()
                .map_err(|_| Error::Cancelled)?;
        }
        if icode_mode == "release" && !icode_release.trim().is_empty() {
            let rel = icode_release.trim();
            if !rel.starts_with("http://") && !rel.starts_with("https://") {
                crate::prepare::icode_paths::set_release(rel)?;
            }
        }
        let m_idx = eval_catalog::MODELS
            .iter()
            .position(|m| *m == model)
            .unwrap_or(0);
        model = eval_catalog::MODELS[Select::with_theme(&theme)
            .with_prompt("DeepSeek model")
            .items(eval_catalog::MODELS)
            .default(m_idx)
            .interact()
            .map_err(|_| Error::Cancelled)?]
        .to_string();
    }

    if args.stage.is_none() && !args.yes && !atty::is(atty::Stream::Stdin) {
        return Err(Error::Config(
            "not a TTY: pass --local, --stage p0..p8, or --yes (Jenkins) / --yes --local".into(),
        ));
    }

    model = eval_catalog::require_model(&model)?;
    if icode_mode == "git" {
        icode_git_url = eval_catalog::require_icode_git_url(&icode_git_url)?;
        let (kind, git_ref) =
            eval_catalog::require_icode_git_ref_for_kind(&icode_git_ref_kind, &icode_git_ref)?;
        icode_git_ref_kind = kind;
        icode_git_ref = git_ref;
        let local_git =
            args.stage.is_some() || args.local || (!args.yes && atty::is(atty::Stream::Stdin));
        if local_git {
            ensure_git_clone_token(&repo, &icode_git_url, args.yes)?;
        }
    }

    if let Some(stage) = args.stage.as_deref() {
        return run_stage(
            &repo,
            stage,
            n_tasks,
            &icode_mode,
            &icode_release,
            &icode_git_url,
            &icode_git_ref,
            &icode_git_ref_kind,
            &workdir,
            &model,
            &benchmark,
            &task,
        );
    }

    let run_local = if local {
        true
    } else if args.yes {
        false
    } else if atty::is(atty::Stream::Stdin) {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Run locally now (--local)? No = trigger Jenkins job {}",
                eval_job_name(&benchmark)
            ))
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
            &icode_git_url,
            &icode_git_ref,
            &icode_git_ref_kind,
            &workdir,
            &model,
            &benchmark,
            &task,
        );
    }

    trigger_jenkins_one_task(
        config,
        n_tasks,
        &icode_mode,
        &icode_git_url,
        &icode_git_ref,
        &icode_git_ref_kind,
        &model,
        &benchmark,
        &task,
    )
}

/// `--task` wins. If `--benchmark` is set, do not steal a leftover shell `TASK`
/// (e.g. ruff_1 after a LoLBench run). Jenkins still gets TASK from `--task` / prompts.
/// Prompt for a forge PAT when cloning a private repo. Writes gitignored `.env`
/// (mode 600). Never prints the value. `--yes` skips the prompt (`.env` / Jenkins).
fn ensure_git_clone_token(repo: &Path, git_url: &str, skip_prompt: bool) -> Result<()> {
    let spec = eval_catalog::git_pat_spec(git_url)?;
    if let Some(token) = load_git_pat(&spec, repo) {
        std::env::set_var(spec.env_var, &token);
        println!(
            "Using {} from env or .env for git clone (value not printed).",
            spec.env_var
        );
        return Ok(());
    }
    if skip_prompt || !atty::is(atty::Stream::Stdin) {
        println!(
            "No {} yet. Public clone will be tried; private repos need that key in gitignored .env \
             (chmod 600) or Jenkins credential {}.",
            spec.env_var, spec.jenkins_id
        );
        return Ok(());
    }
    let token: String = Password::with_theme(&ColorfulTheme::default())
        .with_prompt(spec.prompt)
        .allow_empty_password(true)
        .interact()
        .map_err(|_| Error::Cancelled)?;
    let token = token.trim().to_string();
    if token.is_empty() {
        println!(
            "Empty PAT: clone will fail fast if the repo is private. Add {} to .env later if needed.",
            spec.env_var
        );
        return Ok(());
    }
    std::env::set_var(spec.env_var, &token);
    let path = dotenv_write_path(repo);
    upsert_dotenv_key(&path, spec.env_var, &token)?;
    println!(
        "Saved {} to {} (mode 600). Value not printed.",
        spec.env_var,
        path.display()
    );
    Ok(())
}

fn load_git_pat(spec: &eval_catalog::GitPatSpec, repo: &Path) -> Option<String> {
    for var in [spec.env_var, spec.env_alias] {
        if let Ok(v) = std::env::var(var) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    for f in dotenv_candidate_files(Some(repo)) {
        if let Some(v) = read_dotenv_keys(&f, &[spec.env_var, spec.env_alias]) {
            return Some(v);
        }
    }
    None
}

fn dotenv_candidate_files(repo: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(p) = std::env::var("MAC_K3D_ENV_FILE") {
        let t = p.trim();
        if !t.is_empty() {
            candidates.push(PathBuf::from(t));
        }
    }
    if let Some(r) = repo {
        candidates.push(r.join(".env"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(".env"));
    }
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".config")
        });
    candidates.push(config_home.join("mac-k3d/.env"));
    candidates
}

fn dotenv_write_path(repo: &Path) -> PathBuf {
    if let Ok(p) = std::env::var("MAC_K3D_ENV_FILE") {
        let t = p.trim();
        if !t.is_empty() {
            return PathBuf::from(t);
        }
    }
    let repo_env = repo.join(".env");
    if repo_env.is_file() {
        return repo_env;
    }
    for f in dotenv_candidate_files(Some(repo)) {
        if f.is_file() {
            return f;
        }
    }
    repo_env
}

fn read_dotenv_keys(path: &Path, want_keys: &[&str]) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut found: Option<String> = None;
    for line in text.lines() {
        if let Some((k, v)) = dotenv_line_value(line, want_keys) {
            if k == want_keys[0] {
                return Some(v);
            }
            if found.is_none() {
                found = Some(v);
            }
        }
    }
    found
}

fn dotenv_line_value(line: &str, want_keys: &[&str]) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if !want_keys.iter().any(|k| *k == key) {
        return None;
    }
    let mut value = value.trim().to_string();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value = value[1..value.len() - 1].to_string();
    }
    if value.is_empty() {
        return None;
    }
    Some((key.to_string(), value))
}

fn dotenv_encode(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
    {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn upsert_dotenv_key(path: &Path, key: &str, value: &str) -> Result<()> {
    let mut lines: Vec<String> = if path.is_file() {
        std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("read {}: {e}", path.display())))?
            .lines()
            .map(|s| s.to_string())
            .collect()
    } else {
        Vec::new()
    };
    let prefix = format!("{key}=");
    let new_line = format!("{key}={}", dotenv_encode(value));
    let mut found = false;
    for line in &mut lines {
        let t = line.trim_start();
        if t.starts_with('#') {
            continue;
        }
        if t.starts_with(&prefix) {
            *line = new_line.clone();
            found = true;
            break;
        }
    }
    if !found {
        if lines.last().is_some_and(|l| !l.is_empty()) {
            lines.push(String::new());
        }
        lines.push(new_line);
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Config(format!("mkdir {}: {e}", parent.display())))?;
        }
    }
    let tmp = path.with_file_name(format!(
        ".{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("env")
    ));
    let body = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };
    std::fs::write(&tmp, body)
        .map_err(|e| Error::Config(format!("write {}: {e}", tmp.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path)
        .map_err(|e| Error::Config(format!("replace {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn inherit_eval_task(
    cli_task: Option<String>,
    cli_benchmark: Option<&str>,
    env_task: Option<String>,
) -> String {
    if let Some(t) = cli_task.filter(|s| !s.trim().is_empty()) {
        return t;
    }
    if cli_benchmark.is_some() {
        return String::new();
    }
    env_task
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

fn normalize_benchmark(s: &str) -> Result<String> {
    eval_catalog::require_benchmark(s)
}

fn eval_job_name(benchmark: &str) -> &'static str {
    match benchmark {
        "lolbench" => "lolbench_one_task",
        "swebenchpro" => "swebenchpro_one_task",
        _ => "deepswe_one_task",
    }
}

fn discover_icode_release() -> String {
    crate::prepare::eval_assets::discover_icode_release()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
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
    icode_git_url: &str,
    icode_git_ref: &str,
    icode_git_ref_kind: &str,
    workdir: &Path,
    model: &str,
    benchmark: &str,
    task: &str,
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
        .env("TASK", task)
        .env("ICODE_MODE", icode_mode)
        .env("ICODE_RELEASE", icode_release)
        .env("ICODE_GIT_URL", icode_git_url)
        .env("ICODE_GIT_REF", icode_git_ref)
        .env("ICODE_GIT_REF_KIND", icode_git_ref_kind)
        .env("HARNESS", "icode")
        .env("LLM", "deepseek")
        .env("BENCHMARK", benchmark)
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

fn trigger_jenkins_one_task(
    config: &MacK3dConfig,
    n_tasks: u32,
    icode_mode: &str,
    icode_git_url: &str,
    icode_git_ref: &str,
    icode_git_ref_kind: &str,
    model: &str,
    benchmark: &str,
    task: &str,
) -> Result<()> {
    let url = config
        .jenkins_agent
        .controller_url
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| Some(format!("http://localhost:{}", config.jenkins.host_port)))
        .unwrap();
    let base = url.trim_end_matches('/');
    let job = eval_job_name(benchmark);
    let job_mode = if icode_mode == "git" {
        "git"
    } else {
        "release"
    };
    if job_mode != "git" {
        return Err(Error::Config(format!(
            "Jenkins release mode needs an uploaded ICODE_RELEASE_FILE \
             (GET buildWithParameters cannot attach a file). Open {base}/job/{job}/build, \
             set ICODE_MODE=release, and upload the icode / icode-*-full-* drop. \
             Git mode still queues with --yes: mac-k3d eval --icode-mode git --icode-git-url URL --yes"
        )));
    }
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

    if let Ok(spec) = eval_catalog::git_pat_spec(icode_git_url) {
        println!(
            "Jenkins git clone uses controller credential {} ({}).\n\
             If the repo is private, add it with `mac-k3d config --update-secrets` on the controller.\n\
             Do not put a PAT in ICODE_GIT_URL or job parameters.",
            spec.jenkins_id, spec.env_var
        );
    }
    let build_url = format!(
        "{base}/job/{job}/buildWithParameters?\
         HARNESS=icode&LLM=deepseek&BENCHMARK={}&TASK={}&N_TASKS={n_tasks}\
         &ICODE_MODE={}&ICODE_GIT_URL={}&ICODE_GIT_REF={}&ICODE_GIT_REF_KIND={}&DEEPSEEK_MODEL={}",
        urlencoding_simple(benchmark),
        urlencoding_simple(task),
        urlencoding_simple(job_mode),
        urlencoding_simple(icode_git_url),
        urlencoding_simple(icode_git_ref),
        urlencoding_simple(icode_git_ref_kind),
        urlencoding_simple(model),
    );

    println!("Triggering Jenkins job {job} at {base} …");
    let auth = format!("{user}:{token}");
    let code = jenkins_post_http(&auth, &build_url)?;
    let code = if code == "400" {
        // Job XML rewrite can drop ParametersDefinitionProperty until the next controller config.
        let fallback = format!("{base}/job/{job}/build");
        jenkins_post_http(&auth, &fallback)?
    } else {
        code
    };
    if code != "201" && code != "200" && code != "302" && code != "303" {
        return Err(Error::Config(format!(
            "failed to trigger {job} (HTTP {code}). Ensure the job exists \
             (mac-k3d config on controller) or use --local."
        )));
    }
    println!(
        "Triggered. Watch progress in Jenkins UI:\n  {base}/job/{job}/\n\
         Look for PROGRESS n% lines in the console log."
    );
    let _ = config;
    Ok(())
}

fn jenkins_post_http(auth: &str, url: &str) -> Result<String> {
    let cookie = std::env::temp_dir().join(format!("mac-k3d-eval-crumb-{}", std::process::id()));
    let _ = std::fs::File::create(&cookie);
    let base = url
        .split("/job/")
        .next()
        .unwrap_or(url)
        .trim_end_matches('/');
    let crumb = Command::new("curl")
        .args([
            "-fsS",
            "-b",
            &cookie.display().to_string(),
            "-c",
            &cookie.display().to_string(),
            "-u",
            auth,
            &format!("{base}/crumbIssuer/api/json"),
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let text = String::from_utf8_lossy(&o.stdout);
            let field = extract_json_str(&text, "crumbRequestField")?;
            let value = extract_json_str(&text, "crumb")?;
            Some((field, value))
        });
    let mut args = vec![
        "-sS".into(),
        "-b".into(),
        cookie.display().to_string(),
        "-c".into(),
        cookie.display().to_string(),
        "-u".into(),
        auth.to_string(),
        "-X".into(),
        "POST".into(),
        url.to_string(),
        "-o".into(),
        "/dev/null".into(),
        "-w".into(),
        "%{http_code}".into(),
    ];
    if let Some((field, value)) = crumb {
        args.push("-H".into());
        args.push(format!("{field}: {value}"));
    }
    let status = Command::new("curl").args(&args).output().map_err(|e| {
        let _ = std::fs::remove_file(&cookie);
        Error::CommandFailed {
            cmd: "curl one_task trigger".into(),
            source: e.into(),
        }
    })?;
    let _ = std::fs::remove_file(&cookie);
    Ok(String::from_utf8_lossy(&status.stdout).trim().to_string())
}

fn extract_json_str(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let idx = json.find(&needle)?;
    let after = &json[idx + needle.len()..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let rest = &rest[1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
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
    fn icode_paths_load_empty_without_file() {
        let prev = std::env::var_os("MAC_K3D_ICODE_PATHS");
        let missing =
            std::env::temp_dir().join(format!("mac-k3d-no-icode-paths-{}", std::process::id()));
        let _ = std::fs::remove_file(&missing);
        std::env::set_var("MAC_K3D_ICODE_PATHS", &missing);
        let got = crate::prepare::icode_paths::load();
        match prev {
            Some(v) => std::env::set_var("MAC_K3D_ICODE_PATHS", v),
            None => std::env::remove_var("MAC_K3D_ICODE_PATHS"),
        }
        assert!(got.release.is_empty());
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
        let dir = std::env::temp_dir().join(format!("mac-k3d-eval-full-{}", std::process::id()));
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
        assert!(!crate::prepare::eval_assets::looks_like_root(Path::new(
            "/tmp"
        )));
    }

    #[test]
    fn normalize_benchmark_accepts_lolbench() {
        assert_eq!(normalize_benchmark("lolbench").unwrap(), "lolbench");
        assert_eq!(normalize_benchmark("LOLBENCH").unwrap(), "lolbench");
        assert_eq!(normalize_benchmark("deepswe").unwrap(), "deepswe");
        assert_eq!(normalize_benchmark("swebenchpro").unwrap(), "swebenchpro");
        assert!(normalize_benchmark("other").is_err());
    }

    #[test]
    fn eval_job_name_matches_benchmark() {
        assert_eq!(eval_job_name("lolbench"), "lolbench_one_task");
        assert_eq!(eval_job_name("deepswe"), "deepswe_one_task");
        assert_eq!(eval_job_name("swebenchpro"), "swebenchpro_one_task");
    }

    #[test]
    fn inherit_eval_task_ignores_env_when_benchmark_flag_set() {
        assert_eq!(
            inherit_eval_task(None, Some("deepswe"), Some("ruff_1".into())),
            ""
        );
        assert_eq!(
            inherit_eval_task(
                Some("abs-module-cache-flags".into()),
                Some("deepswe"),
                Some("ruff_1".into())
            ),
            "abs-module-cache-flags"
        );
        assert_eq!(
            inherit_eval_task(None, None, Some("ruff_1".into())),
            "ruff_1"
        );
    }

    #[test]
    fn upsert_dotenv_key_replaces_without_leaking_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, "DEEPSEEK_API_KEY=keep-me\nGITCODE_TOKEN=old\n").unwrap();
        upsert_dotenv_key(&path, "GITCODE_TOKEN", "new-pat-value").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("DEEPSEEK_API_KEY=keep-me"));
        assert!(text.contains("GITCODE_TOKEN=new-pat-value"));
        assert!(!text.contains("GITCODE_TOKEN=old"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "expected 600 got {mode:o}");
        }
        assert_eq!(
            read_dotenv_keys(&path, &["GITCODE_TOKEN", "MAC_K3D_GITCODE_PAT"]).as_deref(),
            Some("new-pat-value")
        );
    }

    #[test]
    fn dotenv_line_value_skips_empty_and_comments() {
        assert!(dotenv_line_value("# GITCODE_TOKEN=x", &["GITCODE_TOKEN"]).is_none());
        assert!(dotenv_line_value("GITCODE_TOKEN=", &["GITCODE_TOKEN"]).is_none());
        assert_eq!(
            dotenv_line_value(r#"GITCODE_TOKEN="abc""#, &["GITCODE_TOKEN"])
                .map(|(_, v)| v)
                .as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn extract_json_str_reads_crumb_fields() {
        let json = r#"{"_class":"hudson.security.csrf.DefaultCrumbIssuer","crumb":"abc123","crumbRequestField":"Jenkins-Crumb"}"#;
        assert_eq!(extract_json_str(json, "crumb").as_deref(), Some("abc123"));
        assert_eq!(
            extract_json_str(json, "crumbRequestField").as_deref(),
            Some("Jenkins-Crumb")
        );
    }
}
