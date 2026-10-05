use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::eval_catalog;

pub const LOLBENCH_ONE_TASK: &str = "lolbench_one_task";

/// Static Jenkins parameter help. ASCII only (XML 1.0 POST). No apostrophes (Groovy singles).
const DESC_ICODE_MODE: &str = "Choose release (upload a drop) or git (clone URL + ref). Fill only the fields for that choice; leave the other group as it is.";
const DESC_ICODE_RELEASE_FILE: &str = "Release: choose the icode / icode-*-full-* drop here. Git: do not choose a file; leave this control as it is. Vice versa: if you chose git, ignore this; if you chose release, this is the file you upload.";
const DESC_ICODE_GIT_URL: &str = "Git: https URL on github.com or gitcode.com. Release: do not change this; leave it as it is. Vice versa: if you chose release, ignore this; if you chose git, this is required.";
const DESC_ICODE_GIT_REF: &str = "Git: branch name, tag, commit SHA, or pull-request number when KIND is pr. Release: do not change this; leave it as it is. Vice versa: if you chose release, ignore this; if you chose git, this is required.";
const DESC_ICODE_GIT_REF_KIND: &str = "Git: pick branch, tag, commit, or pr (no auto). For pr, REF is the pull-request number. Release: do not change this; leave it as it is. Vice versa: if you chose release, ignore this; if you chose git, pick the kind that matches REF.";
const DESC_TASKS: &str = "Comma-separated question ids. This field wins over TASK and N_TASKS. Example: ruff_1,fastapi_1. Leave empty to use TASK or N_TASKS.";
const DESC_CANARY: &str = "Isolation canary (P0.6). official: on when OFFICIAL=1, off otherwise. on: canary on the first task, then the rollouts. only: canary on every selected task and no rollouts (no tokens). off: smoke runs only; OFFICIAL=1 refuses it. A canary failure stops the build.";
const DESC_CANARY_ALLOW_HOST: &str = "Test only: open this host (for example github.com) for the canary alone, to prove the canary fails when isolation is broken. Leave empty. OFFICIAL=1 refuses it.";
const DESC_RESUME: &str = "After agent death or an aborted full suite: set true and keep the same TASK/TASKS/N_TASKS. P5 seeds from the aborted harbor_runs/jenkins-* tree (override with RESUME_FROM=jenkins-N) and continues units that lack reward.json. Works for LoLBench and DeepSWE.";
const DESC_MAC_K3D_GIT_URL: &str = "Git URL of the mac-k3d repo whose pipeline/ runs this build. The build clones it and records the resolved commit in artifact.json, so every result names the pipeline version that produced it.";
const DESC_MAC_K3D_GIT_REF: &str = "Branch name, tag, commit SHA, or pull-request number for MAC_K3D_GIT_URL. Push a fix to a branch and enter it here; to revert a bad commit, enter the previous SHA.";
const DESC_MAC_K3D_GIT_REF_KIND: &str = "Pick branch, tag, commit, or pr to match MAC_K3D_GIT_REF. OFFICIAL=1 requires commit, so an official number always names an exact pipeline revision.";
const DESC_SHARDS: &str = "How many parallel shard builds to split the selected tasks across. Each shard is one some_task build holding its own worker CPU lock. 0 derives it from the registered cores.";
const DESC_RUN_GROUP: &str = "Id that ties the shards of one suite run together. The dispatcher generates it; the aggregator collects every shard carrying the same value. Leave empty on a direct build.";

/// What a job is for. All eval shapes share one Jenkinsfile; only the defaults
/// and the dispatcher/aggregator bodies differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobShape {
    /// One question, one rollout. Smoke test.
    One,
    /// A few questions at the full rollout count. Also the shard runner.
    Some_,
    /// Dispatcher: split the suite into shards and trigger them in parallel.
    FullSuite,
    /// Merge every shard of one RUN_GROUP into a single report.
    Aggregate,
}

impl JobShape {
    fn suffix(self) -> &'static str {
        match self {
            JobShape::One => "one_task",
            JobShape::Some_ => "some_task",
            JobShape::FullSuite => "full_suite_task",
            JobShape::Aggregate => "aggregate",
        }
    }
}

/// `deepswe_one_task`, `lolbench_some_task`, `swebenchpro_full_suite_task`, …
pub fn eval_job_name(job_benchmark: &str, shape: JobShape) -> String {
    if shape == JobShape::Aggregate {
        return EVAL_AGGREGATE.to_string();
    }
    format!("{job_benchmark}_{}", shape.suffix())
}

fn benchmark_label(job_benchmark: &str) -> &'static str {
    match job_benchmark {
        "lolbench" => "LoLBench",
        "deepswe" => "DeepSWE",
        "swebenchpro" => "SWE-bench Pro",
        _ => "this benchmark",
    }
}

/// Jenkins job blurb. Same eval goal on every suite: iCode harness plus an LLM call.
fn job_description(job_benchmark: &str) -> String {
    let label = benchmark_label(job_benchmark);
    let extra = if job_benchmark == "swebenchpro" {
        " The task image is large; use one TASK."
    } else {
        ""
    };
    format!(
        "Evaluate the iCode harness with an LLM call on one {label} task. Harbor runs iCode plus the DeepSeek catalog model, then the same model with no harness.{extra} See docs/user-guide.md."
    )
}

fn shape_description(job_benchmark: &str, shape: JobShape) -> String {
    let label = benchmark_label(job_benchmark);
    let size = full_suite_size(job_benchmark);
    match shape {
        JobShape::One => job_description(job_benchmark),
        JobShape::Some_ => format!(
            "Evaluate the iCode harness on a few {label} questions at the full rollout count. Use it to compare a handful of questions against an earlier result, and as the shard runner the full suite dispatcher triggers. One Harbor run per build. See docs/user-guide.md."
        ),
        JobShape::FullSuite => format!(
            "Run the whole {label} suite ({size} questions) by splitting it into SHARDS parallel {job_benchmark}_some_task builds, then collecting them into one report. This job only dispatches; the shards do the work on whichever workers have free CPU tokens. See docs/harbor-delegation-multiworker/README.md."
        ),
        JobShape::Aggregate => "Merge every shard of one RUN_GROUP into a single artifact.json, summary.md and report.html. Triggered by a full_suite_task build; you can also run it by hand with the same BENCHMARK and RUN_GROUP. See docs/harbor-delegation-multiworker/README.md.".to_string(),
    }
}

fn full_suite_size(job_benchmark: &str) -> u32 {
    match job_benchmark {
        "deepswe" => 113,
        "lolbench" => 20,
        "swebenchpro" => 731,
        _ => 1,
    }
}

fn task_param_description(job_benchmark: &str) -> String {
    match job_benchmark {
        "lolbench" => {
            "One LoLBench question id. A non-empty TASKS list replaces this. Leave empty to use N_TASKS. Example: ruff_1".into()
        }
        "deepswe" => {
            "One DeepSWE question id. A non-empty TASKS list replaces this. Leave empty to use N_TASKS.".into()
        }
        "swebenchpro" => {
            "One SWE-bench Pro instance id. A non-empty TASKS list replaces this. Leave empty to use N_TASKS. The image is large.".into()
        }
        _ => "One question id. A non-empty TASKS list replaces this. Leave empty to use N_TASKS.".into(),
    }
}

fn n_tasks_param_description(job_benchmark: &str) -> String {
    format!(
        "Used only when TASK and TASKS are empty: the first N sorted question ids. Full suite is {}. Set {} with TASK and TASKS empty to run every question.",
        full_suite_size(job_benchmark),
        full_suite_size(job_benchmark)
    )
}

fn benchmark_param_description(job_benchmark: &str) -> String {
    format!(
        "This job uses {} to evaluate the iCode harness with an LLM call. Do not change it.",
        benchmark_label(job_benchmark)
    )
}

/// Options used when rendering / updating `lolbench_one_task`.
#[derive(Debug, Clone)]
pub struct JobOpts {
    pub default_task: String,
    pub default_eval_mode: String,
    pub default_icode_release: String,
    pub default_icode_git_url: String,
    pub default_icode_git_ref: String,
    pub default_icode_git_ref_kind: String,
    /// mac-k3d repo the build clones to get `pipeline/`, and the ref to pin it at.
    pub default_mac_k3d_git_url: String,
    pub default_mac_k3d_git_ref: String,
    pub default_mac_k3d_git_ref_kind: String,
    pub default_icode_args: String,
    pub default_harness: String,
    pub default_llm: String,
    pub default_deepseek_model: String,
    pub default_benchmark: String,
    pub default_n_tasks: u32,
    pub default_n_rollouts: u32,
    pub default_tasks: Vec<String>,
    /// Credential IDs present in Jenkins (only these are bound in the Pipeline).
    pub credential_ids: Vec<String>,
}

impl JobOpts {
    pub fn from_config(config: &crate::config::MacK3dConfig, credential_ids: Vec<String>) -> Self {
        let mut release = config.jenkins_job.default_icode_release.trim().to_string();
        if release.is_empty() {
            // Older YAML used default_binary_target for a raw file path.
            release = config.jenkins_job.default_binary_target.trim().to_string();
        }
        let git_ref = config.jenkins_job.default_icode_git_ref.trim();
        let git_ref_kind = config.jenkins_job.default_icode_git_ref_kind.trim();
        let harness = if config.jenkins_job.default_harness.trim().is_empty() {
            eval_catalog::HARNESSES[0].to_string()
        } else {
            config
                .jenkins_job
                .default_harness
                .trim()
                .to_ascii_lowercase()
        };
        let llm = {
            let live = config.jenkins_job.default_llm.trim();
            let alias = config.jenkins_job.default_model.trim();
            let raw = if live.is_empty() { alias } else { live };
            if raw.is_empty() {
                eval_catalog::LLMS[0].to_string()
            } else {
                raw.to_ascii_lowercase()
            }
        };
        let deepseek_model = {
            let raw = config.jenkins_job.default_deepseek_model.trim();
            if raw.is_empty() {
                eval_catalog::default_model().to_string()
            } else {
                eval_catalog::require_model(raw)
                    .unwrap_or_else(|_| eval_catalog::default_model().to_string())
            }
        };
        let n_rollouts = config.jenkins_job.default_n_rollouts.max(1);
        Self {
            default_task: if config.jenkins_job.default_task.trim().is_empty() {
                String::new()
            } else {
                config.jenkins_job.default_task.clone()
            },
            default_eval_mode: normalize_eval_mode(&config.jenkins_job.default_eval_mode),
            default_icode_release: release,
            default_icode_git_url: config.jenkins_job.default_icode_git_url.clone(),
            default_icode_git_ref: if git_ref.is_empty() {
                "2".into()
            } else {
                git_ref.to_string()
            },
            default_icode_git_ref_kind: match git_ref_kind.to_ascii_lowercase().as_str() {
                "tag" | "commit" | "pr" | "branch" => git_ref_kind.to_ascii_lowercase(),
                _ => "pr".into(),
            },
            default_mac_k3d_git_url: config.jenkins_job.default_mac_k3d_git_url.trim().to_string(),
            default_mac_k3d_git_ref: {
                let r = config.jenkins_job.default_mac_k3d_git_ref.trim();
                if r.is_empty() { "main".into() } else { r.to_string() }
            },
            default_mac_k3d_git_ref_kind: match config
                .jenkins_job
                .default_mac_k3d_git_ref_kind
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "tag" | "commit" | "pr" | "branch" => config
                    .jenkins_job
                    .default_mac_k3d_git_ref_kind
                    .trim()
                    .to_ascii_lowercase(),
                _ => "branch".into(),
            },
            default_icode_args: config.jenkins_job.default_icode_args.clone(),
            default_harness: harness,
            default_llm: llm,
            default_deepseek_model: deepseek_model,
            default_benchmark: config
                .jenkins_job
                .default_benchmark
                .trim()
                .to_ascii_lowercase(),
            default_n_tasks: config.jenkins_job.default_n_tasks.max(1),
            default_n_rollouts: n_rollouts,
            default_tasks: config.jenkins_job.default_tasks.clone(),
            credential_ids,
        }
    }
}

fn normalize_eval_mode(s: &str) -> String {
    match s.trim().to_ascii_lowercase().as_str() {
        "git" | "source" => "git".into(),
        "release" | "binary" | "" => "release".into(),
        other => other.to_string(),
    }
}

fn jenkins_icode_mode(opts: &JobOpts) -> &'static str {
    match opts.default_eval_mode.trim().to_ascii_lowercase().as_str() {
        "git" | "source" => "git",
        _ => "release",
    }
}

/// Ensure Pipeline job `lolbench_one_task` exists on the controller (create or update).
pub fn ensure_lolbench_one_task(
    jenkins_url: &str,
    api_user: &str,
    api_token_or_password: &str,
    opts: &JobOpts,
) -> Result<()> {
    let base = jenkins_url.trim_end_matches('/');
    let auth = format!("{api_user}:{api_token_or_password}");

    wait_for_jenkins(base, &auth, Duration::from_secs(90))?;

    let cookie_file = tempfile_path("mac-k3d-job-cookies")?;
    let crumb = fetch_crumb(base, &auth, &cookie_file);

    let exists = curl_status(
        base,
        &auth,
        &format!("/job/{LOLBENCH_ONE_TASK}/api/json"),
        &crumb,
        &cookie_file,
    )
    .map(|c| c == 200)
    .unwrap_or(false);

    if exists {
        println!("Updating Jenkins job '{LOLBENCH_ONE_TASK}' Pipeline definition…");
        match update_job_script(base, &auth, &crumb, &cookie_file, opts) {
            Ok(()) => println!("Updated job '{LOLBENCH_ONE_TASK}'."),
            Err(err) => println!(
                "Warning: failed to update job '{LOLBENCH_ONE_TASK}' ({err}).\n\
                 Delete the job in the UI and re-run `mac-k3d config`, or see docs/lolbench-jenkins.md."
            ),
        }
        let _ = std::fs::remove_file(&cookie_file);
        return Ok(());
    }

    let xml = job_config_xml(opts);
    let create_url = format!(
        "{base}/createItem?name={}",
        urlencoding_simple(LOLBENCH_ONE_TASK)
    );
    println!("Creating Jenkins job '{LOLBENCH_ONE_TASK}' on {base} …");

    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS",
        "-b",
        &cookie_file.display().to_string(),
        "-c",
        &cookie_file.display().to_string(),
        "-u",
        &auth,
        "-H",
        "Content-Type: text/xml",
        "-X",
        "POST",
        &create_url,
        "--data-binary",
        &xml,
        "-w",
        "\n%{http_code}",
    ]);
    if let Some((field, value)) = &crumb {
        cmd.args(["-H", &format!("{field}: {value}")]);
    }

    let output = cmd.output().map_err(|e| Error::CommandFailed {
        cmd: "curl createItem lolbench_one_task".into(),
        source: e.into(),
    })?;
    let _ = std::fs::remove_file(&cookie_file);

    let raw = String::from_utf8_lossy(&output.stdout);
    let code = raw.lines().last().unwrap_or("").trim().to_string();
    if code != "200" && code != "201" && code != "302" && code != "303" {
        let body: String = raw
            .lines()
            .rev()
            .skip(1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        let snippet: String = body.chars().take(280).collect();
        println!(
            "Warning: failed to create job '{LOLBENCH_ONE_TASK}' (HTTP {code}). {snippet}\n\
             Create it manually — see docs/lolbench-jenkins.md."
        );
        return Ok(());
    }

    println!(
        "Created job '{LOLBENCH_ONE_TASK}'.\n\
         Trigger: {base}/job/{LOLBENCH_ONE_TASK}/buildWithParameters"
    );
    Ok(())
}

fn update_job_script(
    base: &str,
    auth: &str,
    crumb: &Option<(String, String)>,
    cookie_file: &Path,
    opts: &JobOpts,
) -> Result<()> {
    let xml_ok = update_job_config_xml(base, auth, crumb, cookie_file, opts).is_ok();
    if xml_ok {
        return Ok(());
    }
    let script = jenkinsfile(opts);
    let b64 = base64_encode(script.as_bytes());
    let groovy = format!(
        r#"
import jenkins.model.Jenkins
import org.jenkinsci.plugins.workflow.job.WorkflowJob
import org.jenkinsci.plugins.workflow.cps.CpsFlowDefinition
import java.util.Base64
def name = '{name}'
def body = new String(Base64.decoder.decode('{b64}'), 'UTF-8')
def job = Jenkins.instance.getItemByFullName(name, WorkflowJob.class)
if (job == null) {{
  throw new IllegalStateException('missing job ' + name)
}}
job.setDefinition(new CpsFlowDefinition(body, true))
job.save()
println('updated-script')
"#,
        name = LOLBENCH_ONE_TASK,
        b64 = b64,
    );

    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS",
        "-b",
        &cookie_file.display().to_string(),
        "-c",
        &cookie_file.display().to_string(),
        "-u",
        auth,
        "-X",
        "POST",
        &format!("{base}/scriptText"),
        "--data-urlencode",
        &format!("script={groovy}"),
    ]);
    if let Some((field, value)) = crumb {
        cmd.args(["-H", &format!("{field}: {value}")]);
    }
    let output = cmd.output().map_err(|e| Error::CommandFailed {
        cmd: "curl scriptText update job".into(),
        source: e.into(),
    })?;
    let body = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.status.success() || !body.contains("updated-script") {
        return Err(Error::Config(truncate(&body, 300)));
    }
    Ok(())
}

fn update_job_config_xml(
    base: &str,
    auth: &str,
    crumb: &Option<(String, String)>,
    cookie_file: &Path,
    opts: &JobOpts,
) -> Result<()> {
    let xml = job_config_xml(opts);
    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS",
        "-b",
        &cookie_file.display().to_string(),
        "-c",
        &cookie_file.display().to_string(),
        "-u",
        auth,
        "-H",
        "Content-Type: text/xml",
        "-X",
        "POST",
        &format!("{base}/job/{LOLBENCH_ONE_TASK}/config.xml"),
        "--data-binary",
        &xml,
        "-w",
        "\n%{http_code}",
    ]);
    if let Some((field, value)) = crumb {
        cmd.args(["-H", &format!("{field}: {value}")]);
    }
    let output = cmd.output().map_err(|e| Error::CommandFailed {
        cmd: "curl job config.xml".into(),
        source: e.into(),
    })?;
    let raw = String::from_utf8_lossy(&output.stdout);
    let code = raw.lines().last().unwrap_or("").trim().to_string();
    if code != "200" && code != "201" && code != "204" {
        return Err(Error::Config(format!("config.xml HTTP {code}")));
    }
    Ok(())
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        format!("{t}…")
    } else {
        t
    }
}

/// Create/update the job using the in-cluster Jenkins admin password from kubectl.
pub async fn ensure_lolbench_one_task_from_cluster(
    kubectl: &Path,
    config: &crate::config::MacK3dConfig,
    credential_ids: Vec<String>,
) -> Result<()> {
    if !config.jenkins.enabled {
        return Ok(());
    }
    let url = crate::runtime::jenkins::ui_url(config);
    let password = match crate::runtime::jenkins::admin_password(kubectl, config).await {
        Ok(p) if !p.is_empty() => p,
        Ok(_) | Err(_) => {
            println!(
                "Skipping '{LOLBENCH_ONE_TASK}' create — could not read Jenkins admin password yet."
            );
            return Ok(());
        }
    };
    let opts = JobOpts::from_config(config, credential_ids);
    ensure_lolbench_one_task(&url, "admin", &password, &opts)
}

fn wait_for_jenkins(base: &str, auth: &str, timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    loop {
        let cookie = tempfile_path("mac-k3d-job-wait")?;
        let ok = Command::new("curl")
            .args([
                "-fsS",
                "-o",
                "/dev/null",
                "-u",
                auth,
                "-b",
                &cookie.display().to_string(),
                "-c",
                &cookie.display().to_string(),
                &format!("{base}/api/json"),
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let _ = std::fs::remove_file(&cookie);
        if ok {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            return Err(Error::Config(format!(
                "Jenkins at {base} not reachable within {}s (needed to create '{LOLBENCH_ONE_TASK}')",
                timeout.as_secs()
            )));
        }
        thread::sleep(Duration::from_secs(2));
    }
}

fn groovy_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[allow(dead_code)]
fn eval_mode_choices(default_mode: &str) -> (&'static str, &'static str) {
    if default_mode == "git" {
        ("git", "release")
    } else {
        ("release", "git")
    }
}

/// Bind listed Jenkins credential IDs into the Pipeline. Empty IDs omit the block
/// (`--skip-secrets` / `start` must pass IDs from `existing_ids_on_controller`, not `[]`).
fn with_credentials_block(credential_ids: &[String]) -> (String, String) {
    use crate::prepare::jenkins_credentials::CREDENTIAL_DEFS;
    let mut binds = Vec::new();
    for def in CREDENTIAL_DEFS {
        if credential_ids.iter().any(|id| id == def.id) {
            binds.push(format!(
                "string(credentialsId: '{}', variable: '{}')",
                def.id, def.env_var
            ));
        }
    }
    if binds.is_empty() {
        return (String::new(), String::new());
    }
    let open = format!("          withCredentials([{}]) {{\n", binds.join(", "));
    let close = "          }\n".to_string();
    (open, close)
}

fn job_config_xml(opts: &JobOpts) -> String {
    one_task_job_xml("lolbench", &job_description("lolbench"), opts)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn question_defaults_for_job(opts: &JobOpts, job_benchmark: &str) -> (String, u32, String) {
    let _ = job_benchmark;
    let tasks = opts.default_tasks.join(",");
    let n = if opts.default_tasks.is_empty() {
        opts.default_n_tasks.max(1)
    } else {
        opts.default_tasks.len() as u32
    };
    let task = if opts.default_tasks.is_empty() {
        opts.default_task.clone()
    } else {
        String::new()
    };
    (task, n, tasks)
}

/// TASK / N_TASKS / TASKS / N_ROLLOUTS defaults for one shape.
///
/// `one_task` pins a single question at one rollout so a smoke run costs one
/// agent call. `some_task` clears TASK so N_TASKS decides, and keeps the full
/// rollout count. `full_suite_task` defaults N_TASKS to the whole suite.
fn shape_defaults(opts: &JobOpts, job_benchmark: &str, shape: JobShape) -> (String, u32, String, u32) {
    let (task, n_tasks, tasks) = question_defaults_for_job(opts, job_benchmark);
    let rollouts = opts.default_n_rollouts.max(1);
    match shape {
        JobShape::One => (task, 1, tasks, 1),
        JobShape::Some_ => (String::new(), n_tasks.max(2), tasks, rollouts),
        JobShape::FullSuite => (
            String::new(),
            full_suite_size(job_benchmark),
            String::new(),
            rollouts,
        ),
        JobShape::Aggregate => (String::new(), n_tasks, tasks, rollouts),
    }
}

fn groovy_quoted_list(items: &[&str]) -> String {
    items
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn xml_choice_strings(items: &[&str]) -> String {
    items
        .iter()
        .map(|s| format!("              <string>{}</string>", xml_escape(s)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn one_task_jenkinsfile(job_benchmark: &str, opts: &JobOpts) -> String {
    eval_jenkinsfile(job_benchmark, JobShape::One, opts)
}

fn eval_jenkinsfile(job_benchmark: &str, shape: JobShape, opts: &JobOpts) -> String {
    match shape {
        JobShape::FullSuite => return dispatcher_jenkinsfile(job_benchmark, opts),
        JobShape::Aggregate => return aggregate_jenkinsfile(opts),
        _ => {}
    }
    let (cred_open, cred_close) = with_credentials_block(&opts.credential_ids);
    let bench = groovy_escape(job_benchmark);
    let (task, n_tasks, tasks, n_rollouts) = shape_defaults(opts, job_benchmark, shape);
    let task = groovy_escape(&task);
    let tasks = groovy_escape(&tasks);
    let harness_choices =
        eval_catalog::choices_preferred_first(eval_catalog::HARNESSES, &opts.default_harness);
    let llm_choices = eval_catalog::choices_preferred_first(eval_catalog::LLMS, &opts.default_llm);
    let model_choices =
        eval_catalog::choices_preferred_first(eval_catalog::MODELS, &opts.default_deepseek_model);
    let harness_g = groovy_quoted_list(&harness_choices);
    let llm_g = groovy_quoted_list(&llm_choices);
    let model_g = groovy_quoted_list(&model_choices);
    let icode_mode_choices = eval_catalog::choices_preferred_first(
        eval_catalog::ICODE_CI_MODES,
        jenkins_icode_mode(opts),
    );
    let icode_mode_g = groovy_quoted_list(&icode_mode_choices);
    let icode_git_url_g = groovy_escape(opts.default_icode_git_url.trim());
    let icode_git_ref_g = groovy_escape(&opts.default_icode_git_ref);
    let icode_git_ref_kind_choices = eval_catalog::choices_preferred_first(
        eval_catalog::ICODE_GIT_REF_KINDS,
        &opts.default_icode_git_ref_kind,
    );
    let icode_git_ref_kind_g = groovy_quoted_list(&icode_git_ref_kind_choices);
    let mac_k3d_git_url_g = groovy_escape(opts.default_mac_k3d_git_url.trim());
    let mac_k3d_git_ref_g = groovy_escape(&opts.default_mac_k3d_git_ref);
    let mac_k3d_kind_g = groovy_quoted_list(&eval_catalog::choices_preferred_first(
        eval_catalog::ICODE_GIT_REF_KINDS,
        &opts.default_mac_k3d_git_ref_kind,
    ));
    let mac_k3d_git_url_desc_g = groovy_escape(DESC_MAC_K3D_GIT_URL);
    let mac_k3d_git_ref_desc_g = groovy_escape(DESC_MAC_K3D_GIT_REF);
    let mac_k3d_kind_desc_g = groovy_escape(DESC_MAC_K3D_GIT_REF_KIND);
    let run_group_desc_g = groovy_escape(DESC_RUN_GROUP);
    let task_desc_g = groovy_escape(&task_param_description(job_benchmark));
    let bench_desc_g = groovy_escape(&benchmark_param_description(job_benchmark));
    let icode_mode_desc_g = groovy_escape(DESC_ICODE_MODE);
    let icode_release_file_desc_g = groovy_escape(DESC_ICODE_RELEASE_FILE);
    let icode_git_url_desc_g = groovy_escape(DESC_ICODE_GIT_URL);
    let icode_git_ref_desc_g = groovy_escape(DESC_ICODE_GIT_REF);
    let icode_git_ref_kind_desc_g = groovy_escape(DESC_ICODE_GIT_REF_KIND);
    let tasks_desc_g = groovy_escape(DESC_TASKS);
    let resume_desc_g = groovy_escape(DESC_RESUME);
    let canary_desc_g = groovy_escape(DESC_CANARY);
    let canary_allow_desc_g = groovy_escape(DESC_CANARY_ALLOW_HOST);
    let n_tasks_desc_g = groovy_escape(&n_tasks_param_description(job_benchmark));
    format!(
        r#"pipeline {{
  agent {{ label params.AGENT_LABEL }}

  options {{
    timeout(time: 96, unit: 'HOURS')
  }}

  parameters {{
    choice(name: 'HARNESS', choices: [{harness_g}], description: 'Eval harness (catalog; v1: icode)')
    choice(name: 'LLM', choices: [{llm_g}], description: 'LLM family (catalog; v1: deepseek)')
    choice(name: 'BENCHMARK', choices: ['{bench}'], description: '{bench_desc_g}')
    string(name: 'TASK', defaultValue: '{task}', description: '{task_desc_g}')
    string(name: 'TASKS', defaultValue: '{tasks}', description: '{tasks_desc_g}')
    string(name: 'N_TASKS', defaultValue: '{n_tasks}', description: '{n_tasks_desc_g}')
    string(name: 'N_ROLLOUTS', defaultValue: '{n_rollouts}', description: 'Number of iCode attempts for the selected question. Default 4. Each extra attempt is another full agent run.')
    booleanParam(name: 'RESUME', defaultValue: false, description: '{resume_desc_g}')
    choice(name: 'ICODE_MODE', choices: [{icode_mode_g}], description: '{icode_mode_desc_g}')
    stashedFile(name: 'ICODE_RELEASE_FILE', description: '{icode_release_file_desc_g}')
    string(name: 'ICODE_GIT_URL', defaultValue: '{icode_git_url_g}', description: '{icode_git_url_desc_g}')
    string(name: 'ICODE_GIT_REF', defaultValue: '{icode_git_ref_g}', description: '{icode_git_ref_desc_g}')
    choice(name: 'ICODE_GIT_REF_KIND', choices: [{icode_git_ref_kind_g}], description: '{icode_git_ref_kind_desc_g}')
    string(name: 'MAC_K3D_GIT_URL', defaultValue: '{mac_k3d_git_url_g}', description: '{mac_k3d_git_url_desc_g}')
    string(name: 'MAC_K3D_GIT_REF', defaultValue: '{mac_k3d_git_ref_g}', description: '{mac_k3d_git_ref_desc_g}')
    choice(name: 'MAC_K3D_GIT_REF_KIND', choices: [{mac_k3d_kind_g}], description: '{mac_k3d_kind_desc_g}')
    string(name: 'RUN_GROUP', defaultValue: '', description: '{run_group_desc_g}')
    string(name: 'TASK_OFFSET', defaultValue: '0', description: 'Skip this many sorted question ids before taking N_TASKS. The full suite dispatcher uses it to give each shard a disjoint slice; leave 0 otherwise.')
    string(name: 'AGENT_LABEL', defaultValue: 'lolbench')
    string(name: 'CPU_LOCK_QTY', defaultValue: '4', description: 'CPU cores this build reserves on its own worker. Harbor gives each trial the cpus its task.toml declares, so this divided by that number is how many trials run at once. The lock is held for the rollouts only.')
    string(name: 'HARBOR_VERSION', defaultValue: '0.22.0', description: 'Harbor version installed in P1. A mismatch fails the stage.')
    string(name: 'DEEPSWE_REF', defaultValue: '0b9fabbb63b9104d678fe965e1632f2dd9eaa2ea', description: 'DeepSWE commit pinned by P2.')
    string(name: 'LOLBENCH_REF', defaultValue: '1b10d10bb4a10cea54374ac34b8f76b69dc8ce75', description: 'LoLBench commit pinned by P2.')
    string(name: 'ICODE_EXPECT_SHA', defaultValue: '', description: 'When set, P3 fails unless the iCode checkout SHA equals this value.')
    string(name: 'OFFICIAL', defaultValue: '0', description: '1 requires full provenance and the pinned iCode commit. 0 is a smoke run.')
    choice(name: 'CANARY', choices: ['official', 'only', 'on', 'off'], description: '{canary_desc_g}')
    string(name: 'CANARY_ALLOW_HOST', defaultValue: '', description: '{canary_allow_desc_g}')
    choice(name: 'DEEPSEEK_MODEL', choices: [{model_g}], description: 'DeepSeek Chat Completions model id (catalog)')
  }}

  stages {{
    stage('Prepare') {{
      steps {{
{cred_open}          script {{
            try {{
              unstash 'ICODE_RELEASE_FILE'
            }} catch (Throwable t) {{
              echo "No ICODE_RELEASE_FILE stash (${{t}})"
            }}
          }}
          sh '''
{bootstrap}
            echo "PROGRESS 10% P0-P4 prepare (no CPU lock held)"
            MAC_K3D_PHASE=prepare bash "$MAC_K3D_ROOT/pipeline/stages/run_all.sh"
          '''
{cred_close}      }}
    }}

    stage('Evaluate') {{
      steps {{
{cred_open}          script {{
            // Lock this worker's own cores. The resources are named
            // <node>-core-N and carry the node name as a label, so two workers
            // never draw from one pool. Held for the rollouts only.
            lock(label: env.NODE_NAME, quantity: params.CPU_LOCK_QTY as Integer, resource: null) {{
              sh '''
{bootstrap}
                echo "PROGRESS 40% P5 canary + the Harbor run (holding $CPU_LOCK_QTY cores on $NODE_NAME)"
                MAC_K3D_PHASE=evaluate bash "$MAC_K3D_ROOT/pipeline/stages/run_all.sh"
              '''
            }}
          }}
{cred_close}      }}
    }}

    stage('Report') {{
      steps {{
{cred_open}          sh '''
{bootstrap}
            echo "PROGRESS 80% P7-P8 score and report (lock released)"
            MAC_K3D_PHASE=report bash "$MAC_K3D_ROOT/pipeline/stages/run_all.sh"
            echo "PROGRESS 100% done"
            if [ -f "$MAC_K3D_EVAL_WORKDIR/last_output.txt" ]; then
              echo "RESULT $(cat "$MAC_K3D_EVAL_WORKDIR/last_output.txt")"
            fi
          '''
{cred_close}      }}
    }}
  }}

  post {{
    always {{
      script {{
        if (fileExists('eval-runs/last_output.txt')) {{
          def pattern = readFile('eval-runs/last_output.txt').trim()
          if (pattern) {{
            archiveArtifacts artifacts: pattern, allowEmptyArchive: true
          }}
        }}
        // A shard archives its trials so eval_aggregate can pull them by RUN_GROUP.
        if (params.RUN_GROUP?.trim()) {{
          archiveArtifacts artifacts: 'eval-runs/harness/harbor_runs/**', allowEmptyArchive: true
          archiveArtifacts artifacts: 'eval-runs/selected_tasks.txt, eval-runs/eval_protocol_inputs.json, eval-runs/eval_resources.json', allowEmptyArchive: true
        }}
      }}
    }}
  }}
}}
"#,
        bench = bench,
        task = task,
        tasks = tasks,
        n_tasks = n_tasks,
        n_rollouts = n_rollouts,
        harness_g = harness_g,
        llm_g = llm_g,
        model_g = model_g,
        cred_open = cred_open,
        cred_close = cred_close,
        bootstrap = eval_bootstrap_sh(job_benchmark, opts, 12),
        icode_mode_g = icode_mode_g,
        icode_git_url_g = icode_git_url_g,
        icode_git_ref_g = icode_git_ref_g,
        icode_git_ref_kind_g = icode_git_ref_kind_g,
        mac_k3d_git_url_g = mac_k3d_git_url_g,
        mac_k3d_git_ref_g = mac_k3d_git_ref_g,
        mac_k3d_kind_g = mac_k3d_kind_g,
        mac_k3d_git_url_desc_g = mac_k3d_git_url_desc_g,
        mac_k3d_git_ref_desc_g = mac_k3d_git_ref_desc_g,
        mac_k3d_kind_desc_g = mac_k3d_kind_desc_g,
        run_group_desc_g = run_group_desc_g,
        task_desc_g = task_desc_g,
        bench_desc_g = bench_desc_g,
        icode_mode_desc_g = icode_mode_desc_g,
        icode_release_file_desc_g = icode_release_file_desc_g,
        icode_git_url_desc_g = icode_git_url_desc_g,
        icode_git_ref_desc_g = icode_git_ref_desc_g,
        icode_git_ref_kind_desc_g = icode_git_ref_kind_desc_g,
        tasks_desc_g = tasks_desc_g,
        resume_desc_g = resume_desc_g,
        n_tasks_desc_g = n_tasks_desc_g,
    )
}

/// Full-suite dispatcher. Splits the suite into shards and triggers one
/// `<suite>_some_task` build per shard in parallel, then the aggregator.
/// It holds no CPU lock and runs no Docker: the shards do the work wherever
/// Jenkins finds free tokens, which is what makes adding a worker enough.
fn dispatcher_jenkinsfile(job_benchmark: &str, opts: &JobOpts) -> String {
    let bench = groovy_escape(job_benchmark);
    let size = full_suite_size(job_benchmark);
    let shard_job = eval_job_name(job_benchmark, JobShape::Some_);
    let n_rollouts = opts.default_n_rollouts.max(1);
    let model_choices =
        eval_catalog::choices_preferred_first(eval_catalog::MODELS, &opts.default_deepseek_model);
    let model_g = groovy_quoted_list(&model_choices);
    let icode_mode_g = groovy_quoted_list(&eval_catalog::choices_preferred_first(
        eval_catalog::ICODE_CI_MODES,
        jenkins_icode_mode(opts),
    ));
    let icode_git_url_g = groovy_escape(opts.default_icode_git_url.trim());
    let icode_git_ref_g = groovy_escape(&opts.default_icode_git_ref);
    let icode_kind_g = groovy_quoted_list(&eval_catalog::choices_preferred_first(
        eval_catalog::ICODE_GIT_REF_KINDS,
        &opts.default_icode_git_ref_kind,
    ));
    let mac_k3d_git_url_g = groovy_escape(opts.default_mac_k3d_git_url.trim());
    let mac_k3d_git_ref_g = groovy_escape(&opts.default_mac_k3d_git_ref);
    let mac_k3d_kind_g = groovy_quoted_list(&eval_catalog::choices_preferred_first(
        eval_catalog::ICODE_GIT_REF_KINDS,
        &opts.default_mac_k3d_git_ref_kind,
    ));
    format!(
        r#"pipeline {{
  agent none

  options {{
    timeout(time: 96, unit: 'HOURS')
  }}

  parameters {{
    choice(name: 'BENCHMARK', choices: ['{bench}'], description: '{bench_desc_g}')
    string(name: 'N_TASKS', defaultValue: '{size}', description: 'How many questions of the suite to run, lowest ids first. The full suite is {size}; lower it for a rehearsal.')
    string(name: 'TASKS', defaultValue: '', description: '{tasks_desc_g}')
    string(name: 'N_ROLLOUTS', defaultValue: '{n_rollouts}', description: 'Attempts per question. Every shard uses the same value.')
    string(name: 'SHARDS', defaultValue: '0', description: '{shards_desc_g}')
    string(name: 'AGENT_LABEL', defaultValue: 'lolbench', description: 'Label the shard builds ask for. Every worker carrying it is a candidate.')
    string(name: 'CPU_LOCK_QTY', defaultValue: '4', description: 'Cores each shard reserves on the worker that runs it.')
    choice(name: 'ICODE_MODE', choices: [{icode_mode_g}], description: '{icode_mode_desc_g}')
    string(name: 'ICODE_GIT_URL', defaultValue: '{icode_git_url_g}', description: '{icode_git_url_desc_g}')
    string(name: 'ICODE_GIT_REF', defaultValue: '{icode_git_ref_g}', description: '{icode_git_ref_desc_g}')
    choice(name: 'ICODE_GIT_REF_KIND', choices: [{icode_kind_g}], description: '{icode_git_ref_kind_desc_g}')
    string(name: 'MAC_K3D_GIT_URL', defaultValue: '{mac_k3d_git_url_g}', description: '{mac_k3d_git_url_desc_g}')
    string(name: 'MAC_K3D_GIT_REF', defaultValue: '{mac_k3d_git_ref_g}', description: '{mac_k3d_git_ref_desc_g}')
    choice(name: 'MAC_K3D_GIT_REF_KIND', choices: [{mac_k3d_kind_g}], description: '{mac_k3d_kind_desc_g}')
    string(name: 'OFFICIAL', defaultValue: '0', description: '1 requires full provenance and pinned commits. Passed through to every shard.')
    choice(name: 'CANARY', choices: ['official', 'only', 'on', 'off'], description: '{canary_desc_g}')
    choice(name: 'DEEPSEEK_MODEL', choices: [{model_g}], description: 'DeepSeek Chat Completions model id (catalog)')
  }}

  stages {{
    stage('Plan shards') {{
      steps {{
        script {{
          int rollouts = (params.N_ROLLOUTS as Integer)
          int lockQty = (params.CPU_LOCK_QTY as Integer)
          if (rollouts < 1 || lockQty < 1) {{
            error 'N_ROLLOUTS and CPU_LOCK_QTY must be integers >= 1'
          }}
          // Explicit ids win over N_TASKS, exactly as in a shard build.
          List<String> ids = []
          if (params.TASKS?.trim()) {{
            ids = params.TASKS.split(',').collect {{ it.trim() }}.findAll {{ it }}
          }}
          int total = ids ? ids.size() : (params.N_TASKS as Integer)
          if (total < 1) {{
            error 'nothing to run: set N_TASKS >= 1 or a non-empty TASKS list'
          }}
          int shards = (params.SHARDS as Integer)
          if (shards < 1) {{
            // One shard per worker that can take the label, so every worker
            // gets work without the dispatcher knowing how many there are.
            // nodesWithLabel needs pipeline-utility-steps; fall back to 1.
            try {{
              shards = nodesWithLabel(label: params.AGENT_LABEL, offline: false).size()
            }} catch (Throwable t) {{
              echo "Could not count nodes for label ${{params.AGENT_LABEL}} (${{t}}); using SHARDS=1. Set SHARDS explicitly to fan out."
              shards = 1
            }}
            if (shards < 1) {{ shards = 1 }}
          }}
          if (shards > total) {{ shards = total }}
          env.RUN_GROUP = "${{env.JOB_NAME}}-${{env.BUILD_NUMBER}}".replaceAll('[^A-Za-z0-9_.-]', '_')
          echo "RUN_GROUP=${{env.RUN_GROUP}} total=${{total}} shards=${{shards}} rollouts=${{rollouts}}"

          Map<String, Closure> branches = [:]
          for (int i = 0; i < shards; i++) {{
            int index = i
            // Contiguous, non-overlapping slices. With explicit ids the shard
            // gets its own TASKS; otherwise it gets an offset into the suite.
            int from = (int) (((long) total * index) / shards)
            int to = (int) (((long) total * (index + 1)) / shards)
            if (to <= from) {{ continue }}
            String shardTasks = ids ? ids.subList(from, to).join(',') : ''
            int shardCount = to - from
            branches["shard-${{index + 1}}"] = {{
              build job: '{shard_job}',
                wait: true,
                propagate: false,
                parameters: [
                  string(name: 'BENCHMARK', value: params.BENCHMARK),
                  string(name: 'TASK', value: ''),
                  string(name: 'TASKS', value: shardTasks),
                  string(name: 'N_TASKS', value: "${{shardCount}}"),
                  string(name: 'TASK_OFFSET', value: ids ? '0' : "${{from}}"),
                  string(name: 'N_ROLLOUTS', value: "${{rollouts}}"),
                  string(name: 'RUN_GROUP', value: env.RUN_GROUP),
                  string(name: 'AGENT_LABEL', value: params.AGENT_LABEL),
                  string(name: 'CPU_LOCK_QTY', value: "${{lockQty}}"),
                  string(name: 'ICODE_MODE', value: params.ICODE_MODE),
                  string(name: 'ICODE_GIT_URL', value: params.ICODE_GIT_URL),
                  string(name: 'ICODE_GIT_REF', value: params.ICODE_GIT_REF),
                  string(name: 'ICODE_GIT_REF_KIND', value: params.ICODE_GIT_REF_KIND),
                  string(name: 'MAC_K3D_GIT_URL', value: params.MAC_K3D_GIT_URL),
                  string(name: 'MAC_K3D_GIT_REF', value: params.MAC_K3D_GIT_REF),
                  string(name: 'MAC_K3D_GIT_REF_KIND', value: params.MAC_K3D_GIT_REF_KIND),
                  string(name: 'OFFICIAL', value: params.OFFICIAL),
                  string(name: 'CANARY', value: params.CANARY),
                  string(name: 'DEEPSEEK_MODEL', value: params.DEEPSEEK_MODEL),
                ]
            }}
          }}
          env.SHARD_COUNT = "${{branches.size()}}"
          parallel branches
        }}
      }}
    }}

    stage('Aggregate') {{
      steps {{
        script {{
          build job: '{aggregate_job}',
            wait: true,
            propagate: true,
            parameters: [
              string(name: 'BENCHMARK', value: params.BENCHMARK),
              string(name: 'RUN_GROUP', value: env.RUN_GROUP),
              string(name: 'SHARD_JOB', value: '{shard_job}'),
              string(name: 'N_ROLLOUTS', value: params.N_ROLLOUTS),
              string(name: 'AGENT_LABEL', value: params.AGENT_LABEL),
              string(name: 'MAC_K3D_GIT_URL', value: params.MAC_K3D_GIT_URL),
              string(name: 'MAC_K3D_GIT_REF', value: params.MAC_K3D_GIT_REF),
              string(name: 'MAC_K3D_GIT_REF_KIND', value: params.MAC_K3D_GIT_REF_KIND),
            ]
        }}
      }}
    }}
  }}
}}
"#,
        bench = bench,
        size = size,
        n_rollouts = n_rollouts,
        shard_job = groovy_escape(&shard_job),
        aggregate_job = EVAL_AGGREGATE,
        model_g = model_g,
        icode_mode_g = icode_mode_g,
        icode_git_url_g = icode_git_url_g,
        icode_git_ref_g = icode_git_ref_g,
        icode_kind_g = icode_kind_g,
        mac_k3d_git_url_g = mac_k3d_git_url_g,
        mac_k3d_git_ref_g = mac_k3d_git_ref_g,
        mac_k3d_kind_g = mac_k3d_kind_g,
        bench_desc_g = groovy_escape(&benchmark_param_description(job_benchmark)),
        tasks_desc_g = groovy_escape(DESC_TASKS),
        shards_desc_g = groovy_escape(DESC_SHARDS),
        icode_mode_desc_g = groovy_escape(DESC_ICODE_MODE),
        icode_git_url_desc_g = groovy_escape(DESC_ICODE_GIT_URL),
        icode_git_ref_desc_g = groovy_escape(DESC_ICODE_GIT_REF),
        icode_git_ref_kind_desc_g = groovy_escape(DESC_ICODE_GIT_REF_KIND),
        mac_k3d_git_url_desc_g = groovy_escape(DESC_MAC_K3D_GIT_URL),
        mac_k3d_git_ref_desc_g = groovy_escape(DESC_MAC_K3D_GIT_REF),
        mac_k3d_kind_desc_g = groovy_escape(DESC_MAC_K3D_GIT_REF_KIND),
        canary_desc_g = groovy_escape(DESC_CANARY),
    )
}

/// One shared job that merges the shards of a RUN_GROUP into a single report.
/// It copies each shard's archived trials, then runs `aggregate_runs.py`.
fn aggregate_jenkinsfile(opts: &JobOpts) -> String {
    let mac_k3d_git_url_g = groovy_escape(opts.default_mac_k3d_git_url.trim());
    let mac_k3d_git_ref_g = groovy_escape(&opts.default_mac_k3d_git_ref);
    let mac_k3d_kind_g = groovy_quoted_list(&eval_catalog::choices_preferred_first(
        eval_catalog::ICODE_GIT_REF_KINDS,
        &opts.default_mac_k3d_git_ref_kind,
    ));
    format!(
        r#"pipeline {{
  agent {{ label params.AGENT_LABEL }}

  options {{
    timeout(time: 4, unit: 'HOURS')
  }}

  parameters {{
    choice(name: 'BENCHMARK', choices: ['deepswe', 'lolbench', 'swebenchpro'], description: 'Which suite the shards ran.')
    string(name: 'RUN_GROUP', defaultValue: '', description: '{run_group_desc_g}')
    string(name: 'SHARD_JOB', defaultValue: '', description: 'Job the shards ran under, for example deepswe_some_task. Its archived builds are searched for RUN_GROUP.')
    string(name: 'N_ROLLOUTS', defaultValue: '4', description: 'Attempts per question the shards used. Decides the pass@k ladder.')
    string(name: 'AGENT_LABEL', defaultValue: 'lolbench', description: 'Where to run the merge. No Docker and no CPU lock; any worker will do.')
    string(name: 'MAC_K3D_GIT_URL', defaultValue: '{mac_k3d_git_url_g}', description: '{mac_k3d_git_url_desc_g}')
    string(name: 'MAC_K3D_GIT_REF', defaultValue: '{mac_k3d_git_ref_g}', description: '{mac_k3d_git_ref_desc_g}')
    choice(name: 'MAC_K3D_GIT_REF_KIND', choices: [{mac_k3d_kind_g}], description: '{mac_k3d_kind_desc_g}')
  }}

  stages {{
    stage('Collect shards') {{
      steps {{
        script {{
          if (!params.RUN_GROUP?.trim()) {{ error 'RUN_GROUP is required' }}
          if (!params.SHARD_JOB?.trim()) {{ error 'SHARD_JOB is required' }}
          deleteDir()
          copyArtifacts projectName: params.SHARD_JOB,
            selector: specific('*'),
            parameters: "RUN_GROUP=${{params.RUN_GROUP}}",
            filter: 'eval-runs/**',
            target: 'shards',
            flatten: false,
            optional: false
        }}
      }}
    }}

    stage('Merge') {{
      steps {{
        sh '''
          set -euo pipefail
          export PATH="${{HOME}}/.local/bin:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:${{PATH}}"
          MAC_K3D_SRC="${{WORKSPACE}}/mac-k3d-src"
          case "${{MAC_K3D_GIT_REF_KIND:-branch}}" in
            commit) FETCH="${{MAC_K3D_GIT_REF}}" ;;
            tag) FETCH="refs/tags/${{MAC_K3D_GIT_REF}}" ;;
            pr) FETCH="refs/pull/${{MAC_K3D_GIT_REF}}/head" ;;
            *) FETCH="refs/heads/${{MAC_K3D_GIT_REF}}" ;;
          esac
          rm -rf "$MAC_K3D_SRC"
          git -c init.defaultBranch=main -c advice.defaultBranchName=false init -q "$MAC_K3D_SRC"
          git -C "$MAC_K3D_SRC" remote add origin "$MAC_K3D_GIT_URL"
          GIT_TERMINAL_PROMPT=0 git -C "$MAC_K3D_SRC" fetch --depth 1 --force origin "$FETCH"
          git -c advice.detachedHead=false -C "$MAC_K3D_SRC" checkout --force --detach FETCH_HEAD
          echo "mac-k3d pipeline: $(git -C "$MAC_K3D_SRC" rev-parse HEAD)"
          python3 "$MAC_K3D_SRC/pipeline/lib/aggregate_runs.py" \
            --shards "${{WORKSPACE}}/shards" \
            --benchmark "$BENCHMARK" \
            --run-group "$RUN_GROUP" \
            --n-rollouts "${{N_ROLLOUTS:-4}}" \
            --out "${{WORKSPACE}}/aggregate"
        '''
      }}
    }}
  }}

  post {{
    always {{
      archiveArtifacts artifacts: 'aggregate/**', allowEmptyArchive: true
    }}
  }}
}}
"#,
        run_group_desc_g = groovy_escape(DESC_RUN_GROUP),
        mac_k3d_git_url_g = mac_k3d_git_url_g,
        mac_k3d_git_ref_g = mac_k3d_git_ref_g,
        mac_k3d_kind_g = mac_k3d_kind_g,
        mac_k3d_git_url_desc_g = groovy_escape(DESC_MAC_K3D_GIT_URL),
        mac_k3d_git_ref_desc_g = groovy_escape(DESC_MAC_K3D_GIT_REF),
        mac_k3d_kind_desc_g = groovy_escape(DESC_MAC_K3D_GIT_REF_KIND),
    )
}

/// The shell prelude every phase runs: clone the pinned mac-k3d, export the
/// parameters, validate them. Idempotent, so each phase re-derives the same env
/// without the phases having to share a shell.
fn eval_bootstrap_sh(job_benchmark: &str, opts: &JobOpts, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let body = format!(
        r#"set -euo pipefail
export PATH="${{HOME}}/.local/bin:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:${{PATH}}"
command -v docker >/dev/null
docker info >/dev/null
export MAC_K3D_EVAL_WORKDIR="${{WORKSPACE}}/eval-runs"
export OFFICIAL="${{OFFICIAL:-0}}"

# The pipeline under test is a git checkout, not whatever binary happens to be
# on this worker. Every build records the commit it ran.
MAC_K3D_SRC="${{WORKSPACE}}/mac-k3d-src"
[ -n "${{MAC_K3D_GIT_URL:-}}" ] || {{ echo "MAC_K3D_GIT_URL is required: the build has to name the pipeline it runs." >&2; exit 1; }}
case "${{MAC_K3D_GIT_REF_KIND:-branch}}" in
  commit) MAC_K3D_FETCH="${{MAC_K3D_GIT_REF}}" ;;
  tag) MAC_K3D_FETCH="refs/tags/${{MAC_K3D_GIT_REF}}" ;;
  pr) MAC_K3D_FETCH="refs/pull/${{MAC_K3D_GIT_REF}}/head" ;;
  branch | "") MAC_K3D_FETCH="refs/heads/${{MAC_K3D_GIT_REF}}" ;;
  *) echo "MAC_K3D_GIT_REF_KIND must be branch, tag, commit or pr (got ${{MAC_K3D_GIT_REF_KIND}})" >&2; exit 1 ;;
esac
if [ "$OFFICIAL" = "1" ] && [ "${{MAC_K3D_GIT_REF_KIND:-branch}}" != "commit" ]; then
  echo "OFFICIAL=1 needs MAC_K3D_GIT_REF_KIND=commit so the result names an exact pipeline revision." >&2
  exit 1
fi
if [ ! -d "$MAC_K3D_SRC/.git" ]; then
  rm -rf "$MAC_K3D_SRC"
  git -c init.defaultBranch=main -c advice.defaultBranchName=false init -q "$MAC_K3D_SRC"
  git -C "$MAC_K3D_SRC" remote add origin "$MAC_K3D_GIT_URL"
else
  git -C "$MAC_K3D_SRC" remote set-url origin "$MAC_K3D_GIT_URL"
fi
GIT_TERMINAL_PROMPT=0 git -C "$MAC_K3D_SRC" fetch --depth 1 --force origin "$MAC_K3D_FETCH"
git -c advice.detachedHead=false -C "$MAC_K3D_SRC" checkout --force --detach FETCH_HEAD
export MAC_K3D_ROOT="$MAC_K3D_SRC"
MAC_K3D_SHA="$(git -C "$MAC_K3D_SRC" rev-parse HEAD)"
export MAC_K3D_SHA
export MAC_K3D_GIT_URL MAC_K3D_GIT_REF
export MAC_K3D_GIT_REF_KIND="${{MAC_K3D_GIT_REF_KIND:-branch}}"
echo "mac-k3d pipeline: $MAC_K3D_GIT_URL ${{MAC_K3D_GIT_REF_KIND}}=${{MAC_K3D_GIT_REF}} -> $MAC_K3D_SHA"
[ -f "$MAC_K3D_ROOT/pipeline/stages/run_all.sh" ] || {{ echo "$MAC_K3D_GIT_URL at $MAC_K3D_SHA has no pipeline/stages/run_all.sh" >&2; exit 1; }}

export N_TASKS="${{N_TASKS:-1}}"
export N_ROLLOUTS="${{N_ROLLOUTS:-4}}"
export RESUME="${{RESUME:-false}}"
export RESUME_FROM="${{RESUME_FROM:-}}"
export RUN_GROUP="${{RUN_GROUP:-}}"
export TASK_OFFSET="${{TASK_OFFSET:-0}}"
case "$TASK_OFFSET" in
  "" | *[!0-9]*) echo "TASK_OFFSET must be an integer >= 0 (got '$TASK_OFFSET')" >&2; exit 1 ;;
esac
for pair in "N_ROLLOUTS=$N_ROLLOUTS" "N_TASKS=$N_TASKS" "CPU_LOCK_QTY=${{CPU_LOCK_QTY:-4}}"; do
  name="${{pair%%=*}}"
  value="${{pair#*=}}"
  case "$value" in
    "" | *[!0-9]*) echo "$name must be an integer >= 1 (got '$value')" >&2; exit 1 ;;
  esac
  [ "$value" -ge 1 ] || {{ echo "$name must be an integer >= 1 (got '$value')" >&2; exit 1; }}
done
export CPU_LOCK_QTY="${{CPU_LOCK_QTY:-4}}"
export TASK="${{TASK:-}}"
export TASKS="${{TASKS:-}}"
export ICODE_MODE="${{ICODE_MODE:-{icode_mode_fb}}}"
export ICODE_RELEASE=""
export ICODE_RELEASE_UPLOADED=""
if [ "${{ICODE_MODE}}" = "git" ]; then
  bash "$MAC_K3D_ROOT/pipeline/lib/icode_input.sh" --force-rm "${{WORKSPACE}}/ICODE_RELEASE_FILE"
elif [ -s "${{WORKSPACE}}/ICODE_RELEASE_FILE" ]; then
  export ICODE_RELEASE="${{WORKSPACE}}/ICODE_RELEASE_FILE"
  export ICODE_RELEASE_UPLOADED=1
fi
export ICODE_GIT_URL="${{ICODE_GIT_URL:-}}"
export ICODE_GIT_REF="${{ICODE_GIT_REF:-main}}"
export ICODE_GIT_REF_KIND="${{ICODE_GIT_REF_KIND:-branch}}"
export HARBOR_VERSION="${{HARBOR_VERSION:-0.22.0}}"
export DEEPSWE_REF="${{DEEPSWE_REF:-0b9fabbb63b9104d678fe965e1632f2dd9eaa2ea}}"
export LOLBENCH_REF="${{LOLBENCH_REF:-1b10d10bb4a10cea54374ac34b8f76b69dc8ce75}}"
export ICODE_EXPECT_SHA="${{ICODE_EXPECT_SHA:-}}"
export CANARY="${{CANARY:-official}}"
export CANARY_ALLOW_HOST="${{CANARY_ALLOW_HOST:-}}"
export HARNESS="${{HARNESS:-{harness_fb}}}"
export LLM="${{LLM:-{llm_fb}}}"
export BENCHMARK="${{BENCHMARK:-{bench}}}"
export DEEPSEEK_MODEL="${{DEEPSEEK_MODEL:-{model_fb}}}"
export LLM_NAME="${{LLM_NAME:-DeepSeek V4 Pro}}"
if [ -z "${{DEEPSEEK_API_KEY:-}}" ]; then
  echo "DEEPSEEK_API_KEY missing. Store credential deepseek-api-key on the Jenkins controller." >&2
  exit 1
fi
if [ "${{ICODE_MODE}}" = "release" ] || [ "${{ICODE_MODE}}" = "binary" ]; then
  if [ -z "${{ICODE_RELEASE}}" ]; then
    echo "ICODE_MODE=release: upload ICODE_RELEASE_FILE on Jenkins Build with Parameters (not a worker path)." >&2
    exit 1
  fi
fi
echo "MAC_K3D_ROOT=$MAC_K3D_ROOT BENCHMARK=$BENCHMARK ICODE_MODE=$ICODE_MODE""#,
        icode_mode_fb = jenkins_icode_mode(opts),
        harness_fb = eval_catalog::HARNESSES[0],
        llm_fb = eval_catalog::LLMS[0],
        bench = groovy_escape(job_benchmark),
        model_fb = eval_catalog::default_model(),
    );
    body
        .lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("{pad}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn one_task_job_xml(job_benchmark: &str, description: &str, opts: &JobOpts) -> String {
    eval_job_xml(job_benchmark, JobShape::One, description, opts)
}

fn eval_job_xml(
    job_benchmark: &str,
    shape: JobShape,
    description: &str,
    opts: &JobOpts,
) -> String {
    if shape == JobShape::FullSuite || shape == JobShape::Aggregate {
        return dispatch_job_xml(job_benchmark, shape, description, opts);
    }
    let script = eval_jenkinsfile(job_benchmark, shape, opts);
    let (task, n_tasks, tasks, n_rollouts) = shape_defaults(opts, job_benchmark, shape);
    let bench_xml = xml_escape(job_benchmark);
    let task_xml = xml_escape(&task);
    let tasks_xml = xml_escape(&tasks);
    let desc_xml = xml_escape(description);
    let task_desc_xml = xml_escape(&task_param_description(job_benchmark));
    let bench_desc_xml = xml_escape(&benchmark_param_description(job_benchmark));
    let harness_choices =
        eval_catalog::choices_preferred_first(eval_catalog::HARNESSES, &opts.default_harness);
    let llm_choices = eval_catalog::choices_preferred_first(eval_catalog::LLMS, &opts.default_llm);
    let model_choices =
        eval_catalog::choices_preferred_first(eval_catalog::MODELS, &opts.default_deepseek_model);
    let harness_xml = xml_choice_strings(&harness_choices);
    let llm_xml = xml_choice_strings(&llm_choices);
    let model_xml = xml_choice_strings(&model_choices);
    let icode_mode_choices = eval_catalog::choices_preferred_first(
        eval_catalog::ICODE_CI_MODES,
        jenkins_icode_mode(opts),
    );
    let icode_mode_xml = xml_choice_strings(&icode_mode_choices);
    let icode_git_url_xml = xml_escape(opts.default_icode_git_url.trim());
    let icode_git_ref_xml = xml_escape(&opts.default_icode_git_ref);
    let icode_git_ref_kind_choices = eval_catalog::choices_preferred_first(
        eval_catalog::ICODE_GIT_REF_KINDS,
        &opts.default_icode_git_ref_kind,
    );
    let icode_git_kind_xml = xml_choice_strings(&icode_git_ref_kind_choices);
    let mac_k3d_git_url_xml = xml_escape(opts.default_mac_k3d_git_url.trim());
    let mac_k3d_git_ref_xml = xml_escape(&opts.default_mac_k3d_git_ref);
    let mac_k3d_kind_xml = xml_choice_strings(&eval_catalog::choices_preferred_first(
        eval_catalog::ICODE_GIT_REF_KINDS,
        &opts.default_mac_k3d_git_ref_kind,
    ));
    let mac_k3d_git_url_desc_xml = xml_escape(DESC_MAC_K3D_GIT_URL);
    let mac_k3d_git_ref_desc_xml = xml_escape(DESC_MAC_K3D_GIT_REF);
    let mac_k3d_kind_desc_xml = xml_escape(DESC_MAC_K3D_GIT_REF_KIND);
    let run_group_desc_xml = xml_escape(DESC_RUN_GROUP);
    let icode_mode_desc_xml = xml_escape(DESC_ICODE_MODE);
    let icode_release_file_desc_xml = xml_escape(DESC_ICODE_RELEASE_FILE);
    let icode_git_url_desc_xml = xml_escape(DESC_ICODE_GIT_URL);
    let icode_git_ref_desc_xml = xml_escape(DESC_ICODE_GIT_REF);
    let icode_git_ref_kind_desc_xml = xml_escape(DESC_ICODE_GIT_REF_KIND);
    let tasks_desc_xml = xml_escape(DESC_TASKS);
    let resume_desc_xml = xml_escape(DESC_RESUME);
    let canary_desc_xml = xml_escape(DESC_CANARY);
    let canary_allow_desc_xml = xml_escape(DESC_CANARY_ALLOW_HOST);
    let n_tasks_desc_xml = xml_escape(&n_tasks_param_description(job_benchmark));
    format!(
        r#"<?xml version='1.0' encoding='UTF-8'?>
<flow-definition plugin="workflow-job">
  <description>{desc_xml}</description>
  <keepDependencies>false</keepDependencies>
  <properties>
    <hudson.model.ParametersDefinitionProperty>
      <parameterDefinitions>
        <hudson.model.ChoiceParameterDefinition>
          <name>HARNESS</name>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
{harness_xml}
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
        <hudson.model.ChoiceParameterDefinition>
          <name>LLM</name>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
{llm_xml}
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
        <hudson.model.ChoiceParameterDefinition>
          <name>BENCHMARK</name>
          <description>{bench_desc_xml}</description>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
              <string>{bench_xml}</string>
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>TASK</name>
          <description>{task_desc_xml}</description>
          <defaultValue>{task_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>TASKS</name>
          <description>{tasks_desc_xml}</description>
          <defaultValue>{tasks_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>N_TASKS</name>
          <description>{n_tasks_desc_xml}</description>
          <defaultValue>{n_tasks}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>N_ROLLOUTS</name>
          <description>Number of iCode attempts for the selected question. Default 4. Each extra attempt is another full agent run.</description>
          <defaultValue>{n_rollouts}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.BooleanParameterDefinition>
          <name>RESUME</name>
          <description>{resume_desc_xml}</description>
          <defaultValue>false</defaultValue>
        </hudson.model.BooleanParameterDefinition>
        <hudson.model.ChoiceParameterDefinition>
          <name>ICODE_MODE</name>
          <description>{icode_mode_desc_xml}</description>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
{icode_mode_xml}
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
        <io.jenkins.plugins.file_parameters.StashedFileParameterDefinition>
          <name>ICODE_RELEASE_FILE</name>
          <description>{icode_release_file_desc_xml}</description>
        </io.jenkins.plugins.file_parameters.StashedFileParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>ICODE_GIT_URL</name>
          <description>{icode_git_url_desc_xml}</description>
          <defaultValue>{icode_git_url_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>ICODE_GIT_REF</name>
          <description>{icode_git_ref_desc_xml}</description>
          <defaultValue>{icode_git_ref_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.ChoiceParameterDefinition>
          <name>ICODE_GIT_REF_KIND</name>
          <description>{icode_git_ref_kind_desc_xml}</description>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
{icode_git_kind_xml}
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>MAC_K3D_GIT_URL</name>
          <description>{mac_k3d_git_url_desc_xml}</description>
          <defaultValue>{mac_k3d_git_url_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>MAC_K3D_GIT_REF</name>
          <description>{mac_k3d_git_ref_desc_xml}</description>
          <defaultValue>{mac_k3d_git_ref_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.ChoiceParameterDefinition>
          <name>MAC_K3D_GIT_REF_KIND</name>
          <description>{mac_k3d_kind_desc_xml}</description>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
{mac_k3d_kind_xml}
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>RUN_GROUP</name>
          <description>{run_group_desc_xml}</description>
          <defaultValue></defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>TASK_OFFSET</name>
          <description>Skip this many sorted question ids before taking N_TASKS. The full suite dispatcher uses it to give each shard a disjoint slice; leave 0 otherwise.</description>
          <defaultValue>0</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>AGENT_LABEL</name>
          <defaultValue>lolbench</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>CPU_LOCK_QTY</name>
          <description>CPU cores this build reserves on its own worker. Harbor gives each trial the cpus its task.toml declares, so this divided by that number is how many trials run at once. The lock is held for the rollouts only.</description>
          <defaultValue>4</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>HARBOR_VERSION</name>
          <description>Harbor version installed in P1. A mismatch fails the stage.</description>
          <defaultValue>0.22.0</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>DEEPSWE_REF</name>
          <description>DeepSWE commit pinned by P2.</description>
          <defaultValue>0b9fabbb63b9104d678fe965e1632f2dd9eaa2ea</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>LOLBENCH_REF</name>
          <description>LoLBench commit pinned by P2.</description>
          <defaultValue>1b10d10bb4a10cea54374ac34b8f76b69dc8ce75</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>ICODE_EXPECT_SHA</name>
          <description>When set, P3 fails unless the iCode checkout SHA equals this value.</description>
          <defaultValue></defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>OFFICIAL</name>
          <description>1 requires full provenance and the pinned iCode commit. 0 is a smoke run.</description>
          <defaultValue>0</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.ChoiceParameterDefinition>
          <name>CANARY</name>
          <description>{canary_desc_xml}</description>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
              <string>official</string>
              <string>only</string>
              <string>on</string>
              <string>off</string>
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>CANARY_ALLOW_HOST</name>
          <description>{canary_allow_desc_xml}</description>
          <defaultValue></defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.ChoiceParameterDefinition>
          <name>DEEPSEEK_MODEL</name>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
{model_xml}
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
      </parameterDefinitions>
    </hudson.model.ParametersDefinitionProperty>
  </properties>
  <definition class="org.jenkinsci.plugins.workflow.cps.CpsFlowDefinition" plugin="workflow-cps">
    <script><![CDATA[{script}]]></script>
    <sandbox>true</sandbox>
  </definition>
  <triggers/>
  <disabled>false</disabled>
</flow-definition>
"#
    )
}

/// Dispatcher and aggregator config.xml. Both take their parameters from the
/// Jenkinsfile's own `parameters {}` block on the first build, so the XML only
/// has to carry the script plus the few fields the UI needs before that.
fn dispatch_job_xml(
    job_benchmark: &str,
    shape: JobShape,
    description: &str,
    opts: &JobOpts,
) -> String {
    let script = eval_jenkinsfile(job_benchmark, shape, opts);
    let desc_xml = xml_escape(description);
    let bench_xml = xml_escape(job_benchmark);
    let bench_choices = if shape == JobShape::Aggregate {
        "              <string>deepswe</string>\n              <string>lolbench</string>\n              <string>swebenchpro</string>".to_string()
    } else {
        format!("              <string>{bench_xml}</string>")
    };
    let extra = if shape == JobShape::FullSuite {
        format!(
            r#"        <hudson.model.StringParameterDefinition>
          <name>N_TASKS</name>
          <description>{n_tasks_desc}</description>
          <defaultValue>{size}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>SHARDS</name>
          <description>{shards_desc}</description>
          <defaultValue>0</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
"#,
            n_tasks_desc = xml_escape(&n_tasks_param_description(job_benchmark)),
            shards_desc = xml_escape(DESC_SHARDS),
            size = full_suite_size(job_benchmark),
        )
    } else {
        format!(
            r#"        <hudson.model.StringParameterDefinition>
          <name>SHARD_JOB</name>
          <description>Job the shards ran under, for example deepswe_some_task. Its archived builds are searched for RUN_GROUP.</description>
          <defaultValue></defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>RUN_GROUP</name>
          <description>{run_group_desc}</description>
          <defaultValue></defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
"#,
            run_group_desc = xml_escape(DESC_RUN_GROUP),
        )
    };
    format!(
        r#"<?xml version='1.0' encoding='UTF-8'?>
<flow-definition plugin="workflow-job">
  <description>{desc_xml}</description>
  <keepDependencies>false</keepDependencies>
  <properties>
    <hudson.model.ParametersDefinitionProperty>
      <parameterDefinitions>
        <hudson.model.ChoiceParameterDefinition>
          <name>BENCHMARK</name>
          <description>{bench_desc_xml}</description>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
{bench_choices}
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
{extra}        <hudson.model.StringParameterDefinition>
          <name>N_ROLLOUTS</name>
          <description>Attempts per question. Every shard uses the same value.</description>
          <defaultValue>{n_rollouts}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>AGENT_LABEL</name>
          <defaultValue>lolbench</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>MAC_K3D_GIT_URL</name>
          <description>{mac_k3d_git_url_desc_xml}</description>
          <defaultValue>{mac_k3d_git_url_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>MAC_K3D_GIT_REF</name>
          <description>{mac_k3d_git_ref_desc_xml}</description>
          <defaultValue>{mac_k3d_git_ref_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.ChoiceParameterDefinition>
          <name>MAC_K3D_GIT_REF_KIND</name>
          <description>{mac_k3d_kind_desc_xml}</description>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
{mac_k3d_kind_xml}
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
      </parameterDefinitions>
    </hudson.model.ParametersDefinitionProperty>
  </properties>
  <definition class="org.jenkinsci.plugins.workflow.cps.CpsFlowDefinition" plugin="workflow-cps">
    <script><![CDATA[{script}]]></script>
    <sandbox>true</sandbox>
  </definition>
  <triggers/>
  <disabled>false</disabled>
</flow-definition>
"#,
        bench_desc_xml = xml_escape(&benchmark_param_description(job_benchmark)),
        n_rollouts = opts.default_n_rollouts.max(1),
        mac_k3d_git_url_xml = xml_escape(opts.default_mac_k3d_git_url.trim()),
        mac_k3d_git_ref_xml = xml_escape(&opts.default_mac_k3d_git_ref),
        mac_k3d_kind_xml = xml_choice_strings(&eval_catalog::choices_preferred_first(
            eval_catalog::ICODE_GIT_REF_KINDS,
            &opts.default_mac_k3d_git_ref_kind,
        )),
        mac_k3d_git_url_desc_xml = xml_escape(DESC_MAC_K3D_GIT_URL),
        mac_k3d_git_ref_desc_xml = xml_escape(DESC_MAC_K3D_GIT_REF),
        mac_k3d_kind_desc_xml = xml_escape(DESC_MAC_K3D_GIT_REF_KIND),
    )
}

fn jenkinsfile(opts: &JobOpts) -> String {
    one_task_jenkinsfile("lolbench", opts)
}

fn tempfile_path(prefix: &str) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    std::fs::File::create(&path).map_err(|e| Error::Config(e.to_string()))?;
    Ok(path)
}

fn fetch_crumb(base: &str, auth: &str, cookie_file: &Path) -> Option<(String, String)> {
    let output = Command::new("curl")
        .args([
            "-fsS",
            "-b",
            &cookie_file.display().to_string(),
            "-c",
            &cookie_file.display().to_string(),
            "-u",
            auth,
            &format!("{base}/crumbIssuer/api/json"),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let field = extract_json_str(&text, "crumbRequestField")?;
    let crumb = extract_json_str(&text, "crumb")?;
    Some((field, crumb))
}

fn curl_status(
    base: &str,
    auth: &str,
    path: &str,
    crumb: &Option<(String, String)>,
    cookie_file: &Path,
) -> Option<i32> {
    let mut args = vec![
        "-sS".into(),
        "-o".into(),
        "/dev/null".into(),
        "-w".into(),
        "%{http_code}".into(),
        "-b".into(),
        cookie_file.display().to_string(),
        "-c".into(),
        cookie_file.display().to_string(),
        "-u".into(),
        auth.to_string(),
        format!("{base}{path}"),
    ];
    if let Some((field, value)) = crumb {
        args.push("-H".into());
        args.push(format!("{field}: {value}"));
    }
    let output = Command::new("curl").args(&args).output().ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
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
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

pub const DEEPSWE_ONE_TASK: &str = "deepswe_one_task";
pub const SWEBENCHPRO_ONE_TASK: &str = "swebenchpro_one_task";
/// One aggregator shared by every suite; it takes BENCHMARK as a parameter.
pub const EVAL_AGGREGATE: &str = "eval_aggregate";
/// Every suite the job generator knows about.
pub const EVAL_BENCHMARKS: &[&str] = &["deepswe", "lolbench", "swebenchpro"];

/// Create or update every eval job: one / some / full-suite per suite, plus the
/// shared aggregator. Jobs that already exist are rewritten, so running it again
/// after an upgrade is how a controller picks up a new shape.
pub fn ensure_eval_jobs(
    jenkins_url: &str,
    api_user: &str,
    api_token_or_password: &str,
    opts: &JobOpts,
) -> Result<()> {
    for bench in EVAL_BENCHMARKS {
        for shape in [JobShape::One, JobShape::Some_, JobShape::FullSuite] {
            let name = eval_job_name(bench, shape);
            let xml = eval_job_xml(bench, shape, &shape_description(bench, shape), opts);
            ensure_named_one_task_xml(&name, &xml, jenkins_url, api_user, api_token_or_password)?;
        }
    }
    let xml = eval_job_xml(
        "deepswe",
        JobShape::Aggregate,
        &shape_description("deepswe", JobShape::Aggregate),
        opts,
    );
    ensure_named_one_task_xml(
        EVAL_AGGREGATE,
        &xml,
        jenkins_url,
        api_user,
        api_token_or_password,
    )
}

pub async fn ensure_eval_jobs_from_cluster(
    kubectl: &Path,
    config: &crate::config::MacK3dConfig,
    credential_ids: Vec<String>,
) -> Result<()> {
    if !config.jenkins.enabled {
        return Ok(());
    }
    let url = crate::runtime::jenkins::ui_url(config);
    let password = match crate::runtime::jenkins::admin_password(kubectl, config).await {
        Ok(p) if !p.is_empty() => p,
        Ok(_) | Err(_) => {
            println!("Skipping eval job create — could not read Jenkins admin password yet.");
            return Ok(());
        }
    };
    let opts = JobOpts::from_config(config, credential_ids);
    ensure_eval_jobs(&url, "admin", &password, &opts)
}

fn ensure_named_one_task_xml(
    job_name: &str,
    xml: &str,
    jenkins_url: &str,
    api_user: &str,
    api_token_or_password: &str,
) -> Result<()> {
    let base = jenkins_url.trim_end_matches('/');
    let auth = format!("{api_user}:{api_token_or_password}");

    wait_for_jenkins(base, &auth, Duration::from_secs(90))?;

    let cookie_file = tempfile_path(&format!("mac-k3d-{job_name}-cookies"))?;
    let crumb = fetch_crumb(base, &auth, &cookie_file);

    let exists = curl_status(
        base,
        &auth,
        &format!("/job/{job_name}/api/json"),
        &crumb,
        &cookie_file,
    )
    .map(|c| c == 200)
    .unwrap_or(false);

    if exists {
        println!("Updating Jenkins job '{job_name}'…");
        let post_url = format!("{base}/job/{job_name}/config.xml");
        let mut cmd = Command::new("curl");
        cmd.args([
            "-sS",
            "-b",
            &cookie_file.display().to_string(),
            "-c",
            &cookie_file.display().to_string(),
            "-u",
            &auth,
            "-H",
            "Content-Type: text/xml",
            "-X",
            "POST",
            &post_url,
            "--data-binary",
            xml,
            "-w",
            "\n%{http_code}",
        ]);
        if let Some((field, value)) = &crumb {
            cmd.args(["-H", &format!("{field}: {value}")]);
        }
        let output = cmd.output().map_err(|e| Error::CommandFailed {
            cmd: format!("curl config.xml {job_name}"),
            source: e.into(),
        })?;
        let _ = std::fs::remove_file(&cookie_file);
        let raw = String::from_utf8_lossy(&output.stdout);
        let code = raw.lines().last().unwrap_or("").trim().to_string();
        if code != "200" && code != "201" {
            println!("Warning: failed to update '{job_name}' (HTTP {code}).");
        } else {
            println!("Updated job '{job_name}'.");
        }
        return Ok(());
    }

    let create_url = format!("{base}/createItem?name={}", urlencoding_simple(job_name));
    println!("Creating Jenkins job '{job_name}' on {base} …");
    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS",
        "-b",
        &cookie_file.display().to_string(),
        "-c",
        &cookie_file.display().to_string(),
        "-u",
        &auth,
        "-H",
        "Content-Type: text/xml",
        "-X",
        "POST",
        &create_url,
        "--data-binary",
        xml,
        "-w",
        "\n%{http_code}",
    ]);
    if let Some((field, value)) = &crumb {
        cmd.args(["-H", &format!("{field}: {value}")]);
    }
    let output = cmd.output().map_err(|e| Error::CommandFailed {
        cmd: format!("curl createItem {job_name}"),
        source: e.into(),
    })?;
    let _ = std::fs::remove_file(&cookie_file);
    let raw = String::from_utf8_lossy(&output.stdout);
    let code = raw.lines().last().unwrap_or("").trim().to_string();
    if code != "200" && code != "201" && code != "302" && code != "303" {
        println!("Warning: failed to create '{job_name}' (HTTP {code}).");
        return Ok(());
    }
    println!(
        "Created job '{job_name}'.\n\
         Trigger: {base}/job/{job_name}/buildWithParameters  (or: mac-k3d eval)"
    );
    Ok(())
}

/// Ensure Pipeline job `deepswe_one_task` (DeepSWE evaluates the iCode harness with an LLM call).
pub fn ensure_deepswe_one_task(
    jenkins_url: &str,
    api_user: &str,
    api_token_or_password: &str,
    opts: &JobOpts,
) -> Result<()> {
    let xml = deepswe_one_task_job_xml(opts);
    ensure_named_one_task_xml(
        DEEPSWE_ONE_TASK,
        &xml,
        jenkins_url,
        api_user,
        api_token_or_password,
    )
}

/// Ensure Pipeline job `swebenchpro_one_task` (SWE-bench Pro evaluates the iCode harness with an LLM call).
pub fn ensure_swebenchpro_one_task(
    jenkins_url: &str,
    api_user: &str,
    api_token_or_password: &str,
    opts: &JobOpts,
) -> Result<()> {
    let xml = swebenchpro_one_task_job_xml(opts);
    ensure_named_one_task_xml(
        SWEBENCHPRO_ONE_TASK,
        &xml,
        jenkins_url,
        api_user,
        api_token_or_password,
    )
}

pub async fn ensure_deepswe_one_task_from_cluster(
    kubectl: &Path,
    config: &crate::config::MacK3dConfig,
    credential_ids: Vec<String>,
) -> Result<()> {
    if !config.jenkins.enabled {
        return Ok(());
    }
    let url = crate::runtime::jenkins::ui_url(config);
    let password = match crate::runtime::jenkins::admin_password(kubectl, config).await {
        Ok(p) if !p.is_empty() => p,
        Ok(_) | Err(_) => {
            println!(
                "Skipping '{DEEPSWE_ONE_TASK}' create — could not read Jenkins admin password yet."
            );
            return Ok(());
        }
    };
    let opts = JobOpts::from_config(config, credential_ids);
    ensure_deepswe_one_task(&url, "admin", &password, &opts)
}

pub async fn ensure_swebenchpro_one_task_from_cluster(
    kubectl: &Path,
    config: &crate::config::MacK3dConfig,
    credential_ids: Vec<String>,
) -> Result<()> {
    if !config.jenkins.enabled {
        return Ok(());
    }
    let url = crate::runtime::jenkins::ui_url(config);
    let password = match crate::runtime::jenkins::admin_password(kubectl, config).await {
        Ok(p) if !p.is_empty() => p,
        Ok(_) | Err(_) => {
            println!(
                "Skipping '{SWEBENCHPRO_ONE_TASK}' create — could not read Jenkins admin password yet."
            );
            return Ok(());
        }
    };
    let opts = JobOpts::from_config(config, credential_ids);
    ensure_swebenchpro_one_task(&url, "admin", &password, &opts)
}

fn deepswe_one_task_job_xml(opts: &JobOpts) -> String {
    one_task_job_xml("deepswe", &job_description("deepswe"), opts)
}

fn swebenchpro_one_task_job_xml(opts: &JobOpts) -> String {
    one_task_job_xml("swebenchpro", &job_description("swebenchpro"), opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_opts() -> JobOpts {
        JobOpts {
            default_task: "ruff_1".into(),
            default_eval_mode: "binary".into(),
            default_icode_release:
                "https://gitcode.com/example/icode-linux-x86_64-full-v0.1.41.tar.gz".into(),
            default_icode_git_url: "https://gitcode.com/example/icode.git".into(),
            default_icode_git_ref: "main".into(),
            default_icode_git_ref_kind: "branch".into(),
            default_mac_k3d_git_url: "https://github.com/Toby-Yu/mac-k3d.git".into(),
            default_mac_k3d_git_ref: "main".into(),
            default_mac_k3d_git_ref_kind: "branch".into(),
            default_icode_args: "--help".into(),
            default_harness: "icode".into(),
            default_llm: "deepseek".into(),
            default_deepseek_model: eval_catalog::default_model().into(),
            default_benchmark: "lolbench".into(),
            default_n_tasks: 1,
            default_n_rollouts: 4,
            default_tasks: Vec::new(),
            credential_ids: vec!["deepseek-api-key".into(), "gitcode-pat".into()],
        }
    }

    fn deepswe_opts(credential_ids: Vec<String>) -> JobOpts {
        JobOpts {
            default_task: "abs-stepped-slices".into(),
            default_eval_mode: "binary".into(),
            default_icode_release: String::new(),
            default_icode_git_url: String::new(),
            default_icode_git_ref: "main".into(),
            default_icode_git_ref_kind: "branch".into(),
            default_mac_k3d_git_url: "https://github.com/Toby-Yu/mac-k3d.git".into(),
            default_mac_k3d_git_ref: "main".into(),
            default_mac_k3d_git_ref_kind: "branch".into(),
            default_icode_args: String::new(),
            default_harness: "icode".into(),
            default_llm: "deepseek".into(),
            default_deepseek_model: eval_catalog::default_model().into(),
            default_benchmark: "deepswe".into(),
            default_n_tasks: 1,
            default_n_rollouts: 4,
            default_tasks: Vec::new(),
            credential_ids,
        }
    }

    #[test]
    fn lolbench_one_task_uses_shared_pipeline() {
        let jf = jenkinsfile(&sample_opts());
        assert!(jf.contains("pipeline/stages/run_all.sh"));
        assert!(jf.contains("BENCHMARK"));
        assert!(jf.contains("lolbench"));
        assert!(jf.contains("ruff_1"));
        assert!(jf.contains("export TASK="));
        assert!(jf.contains("export HARNESS="));
        assert!(jf.contains("export ICODE_MODE="));
        assert!(jf.contains("ICODE_GIT_URL"));
        assert!(jf.contains("ICODE_GIT_REF"));
        assert!(jf.contains("ICODE_GIT_REF_KIND"));
        assert!(!jf.contains("ICODE_SOURCE"));
        assert!(!jf.contains("'source'"));
        assert!(!jf.contains("string(name: 'ICODE_RELEASE'"));
        assert!(jf.contains("ICODE_RELEASE_FILE"));
        assert!(jf.contains("ICODE_RELEASE_UPLOADED"));
        // The pipeline under test is a clone at a named ref, not whatever
        // binary happens to be installed on the worker.
        assert!(!jf.contains("--sync-pipeline"));
        assert!(!jf.contains("MAC_K3D_ROOT:-"));
        assert!(!jf.contains(".local/share"));
        assert!(jf.contains("MAC_K3D_GIT_URL"));
        assert!(jf.contains("MAC_K3D_GIT_REF"));
        assert!(jf.contains("choice(name: 'MAC_K3D_GIT_REF_KIND'"));
        assert!(jf.contains("MAC_K3D_SHA"));
        assert!(jf.contains("refs/pull/${MAC_K3D_GIT_REF}/head"));
        assert!(jf.contains("OFFICIAL=1 needs MAC_K3D_GIT_REF_KIND=commit"));
        assert!(jf.contains("stashedFile(name: 'ICODE_RELEASE_FILE'"));
        assert!(jf.contains("unstash 'ICODE_RELEASE_FILE'"));
        assert!(jf.contains("--force-rm"));
        assert!(jf.contains(r#"if [ "${ICODE_MODE}" = "git" ]"#));
        assert!(jf.contains("leave this control as it is"));
        assert!(jf.contains("Vice versa"));
        assert!(jf.contains("ICODE_GIT_REF_KIND:-branch"));
        assert!(!jf.contains("ICODE_GIT_REF_KIND:-auto"));
        assert!(jf.contains("HARBOR_VERSION:-0.22.0"));
        assert!(jf.contains("DEEPSWE_REF:-0b9fabbb63b9104d678fe965e1632f2dd9eaa2ea"));
        assert!(jf.contains("LOLBENCH_REF:-1b10d10bb4a10cea54374ac34b8f76b69dc8ce75"));
        assert!(jf.contains("ICODE_EXPECT_SHA:-"));
        assert!(jf.contains("OFFICIAL:-0"));
        assert!(jf.contains("choice(name: 'CANARY', choices: ['official', 'only', 'on', 'off']"));
        assert!(jf.contains("CANARY:-official"));
        assert!(jf.contains("string(name: 'CANARY_ALLOW_HOST', defaultValue: ''"));
        assert!(jf.contains("CANARY_ALLOW_HOST:-"));
        assert!(jf.contains("upload ICODE_RELEASE_FILE"));
        assert!(jf.contains("icode"));
        // Each worker locks its own cores; a shared pool would let a build on
        // worker A hold tokens that belong to worker B.
        assert!(!jf.contains("lock(label: 'CPU_CORES'"));
        assert!(jf.contains("lock(label: env.NODE_NAME"));
        // And it holds them for the rollouts only.
        assert!(jf.contains("stage('Prepare')"));
        assert!(jf.contains("stage('Evaluate')"));
        assert!(jf.contains("stage('Report')"));
        assert!(jf.find("lock(label: env.NODE_NAME").unwrap() > jf.find("stage('Evaluate')").unwrap());
        assert!(jf.find("lock(label: env.NODE_NAME").unwrap() < jf.find("stage('Report')").unwrap());
        assert!(jf.contains("MAC_K3D_PHASE=prepare"));
        assert!(jf.contains("MAC_K3D_PHASE=evaluate"));
        assert!(jf.contains("MAC_K3D_PHASE=report"));
        assert!(jf.contains("withCredentials"));
        assert!(jf.contains("deepseek-api-key"));
        assert!(jf.contains("deepseek-v4-pro"));
        assert!(!jf.contains("harbor run"));
        assert!(!jf.contains("EVAL_MODE"));
        assert!(!jf.contains("icode-in"));
    }

    #[test]
    fn every_suite_gets_three_shapes_plus_one_aggregator() {
        let names: Vec<String> = EVAL_BENCHMARKS
            .iter()
            .flat_map(|b| {
                [JobShape::One, JobShape::Some_, JobShape::FullSuite]
                    .into_iter()
                    .map(move |s| eval_job_name(b, s))
            })
            .collect();
        assert_eq!(
            names,
            vec![
                "deepswe_one_task",
                "deepswe_some_task",
                "deepswe_full_suite_task",
                "lolbench_one_task",
                "lolbench_some_task",
                "lolbench_full_suite_task",
                "swebenchpro_one_task",
                "swebenchpro_some_task",
                "swebenchpro_full_suite_task",
            ]
        );
        // The aggregator is shared; it takes BENCHMARK as a parameter.
        for bench in EVAL_BENCHMARKS {
            assert_eq!(eval_job_name(bench, JobShape::Aggregate), EVAL_AGGREGATE);
        }
    }

    #[test]
    fn shapes_differ_only_in_their_defaults() {
        let mut opts = deepswe_opts(vec!["deepseek-api-key".into()]);
        opts.default_n_tasks = 5;
        opts.default_n_rollouts = 4;
        opts.default_task = "abs-stepped-slices".into();
        let one = eval_jenkinsfile("deepswe", JobShape::One, &opts);
        let some = eval_jenkinsfile("deepswe", JobShape::Some_, &opts);
        // one_task: a single named question, a single attempt.
        assert!(one.contains("string(name: 'TASK', defaultValue: 'abs-stepped-slices'"));
        assert!(one.contains("string(name: 'N_TASKS', defaultValue: '1'"));
        assert!(one.contains("string(name: 'N_ROLLOUTS', defaultValue: '1'"));
        // some_task: N questions at the full rollout count, no pinned TASK.
        assert!(some.contains("string(name: 'TASK', defaultValue: ''"));
        assert!(some.contains("string(name: 'N_TASKS', defaultValue: '5'"));
        assert!(some.contains("string(name: 'N_ROLLOUTS', defaultValue: '4'"));
        // Everything else is the same pipeline.
        assert_eq!(
            one.replace("defaultValue: 'abs-stepped-slices'", "defaultValue: ''")
                .replace("'N_TASKS', defaultValue: '1'", "'N_TASKS', defaultValue: '5'")
                .replace("'N_ROLLOUTS', defaultValue: '1'", "'N_ROLLOUTS', defaultValue: '4'"),
            some
        );
    }

    #[test]
    fn full_suite_dispatches_shards_then_aggregates() {
        let opts = deepswe_opts(vec!["deepseek-api-key".into()]);
        let jf = eval_jenkinsfile("deepswe", JobShape::FullSuite, &opts);
        // The dispatcher does no work itself: no node, no Docker, no lock.
        assert!(jf.contains("agent none"));
        assert!(!jf.contains("lock("));
        assert!(!jf.contains("run_all.sh"));
        assert!(jf.contains("build job: 'deepswe_some_task'"));
        assert!(jf.contains("build job: 'eval_aggregate'"));
        assert!(jf.contains("parallel branches"));
        assert!(jf.contains("env.RUN_GROUP"));
        // Shards take disjoint, contiguous slices of the same sorted id list.
        assert!(jf.contains("TASK_OFFSET"));
        assert!(jf.contains("string(name: 'N_TASKS', defaultValue: '113'"));
        assert!(jf.contains("string(name: 'SHARDS', defaultValue: '0'"));
        // Shard failures must not hide the ones that worked.
        assert!(jf.contains("propagate: false"));

        let lol = eval_jenkinsfile("lolbench", JobShape::FullSuite, &opts);
        assert!(lol.contains("build job: 'lolbench_some_task'"));
        assert!(lol.contains("string(name: 'N_TASKS', defaultValue: '20'"));
    }

    #[test]
    fn aggregate_collects_by_run_group() {
        let opts = deepswe_opts(Vec::new());
        let jf = eval_jenkinsfile("deepswe", JobShape::Aggregate, &opts);
        assert!(jf.contains("copyArtifacts"));
        assert!(jf.contains("RUN_GROUP=${params.RUN_GROUP}"));
        assert!(jf.contains("aggregate_runs.py"));
        assert!(jf.contains("RUN_GROUP is required"));
        // It only merges, so it needs no Docker, no iCode and no lock.
        assert!(!jf.contains("lock("));
        assert!(!jf.contains("ICODE_MODE"));
        assert!(!jf.contains("docker info"));
        // Every suite shares it.
        for bench in EVAL_BENCHMARKS {
            assert!(jf.contains(&format!("<string>{bench}</string>")) || jf.contains(bench));
        }
    }

    #[test]
    fn a_shard_archives_its_trials_for_the_aggregator() {
        let jf = eval_jenkinsfile("deepswe", JobShape::Some_, &deepswe_opts(Vec::new()));
        assert!(jf.contains("params.RUN_GROUP?.trim()"));
        assert!(jf.contains("eval-runs/harness/harbor_runs/**"));
        assert!(jf.contains("eval-runs/selected_tasks.txt"));
    }

    #[test]
    fn job_xml_wraps_pipeline_in_cdata() {
        let xml = job_config_xml(&sample_opts());
        assert!(xml.contains("<![CDATA["));
        assert!(xml.contains("flow-definition"));
        assert!(xml.contains("<name>TASK</name>"));
        assert!(xml.contains("<name>ICODE_MODE</name>"));
        assert!(xml.contains("<name>BENCHMARK</name>"));
        assert!(xml.contains("<string>lolbench</string>"));
        assert!(xml.contains("<defaultValue>ruff_1</defaultValue>"));
        assert!(xml.contains("<string>release</string>"));
        assert!(xml.contains("<name>ICODE_GIT_URL</name>"));
        assert!(xml.contains("<name>ICODE_GIT_REF</name>"));
        assert!(xml.contains("<name>ICODE_GIT_REF_KIND</name>"));
        assert!(xml.contains("<name>ICODE_RELEASE_FILE</name>"));
        assert!(xml.contains("<name>HARBOR_VERSION</name>"));
        assert!(xml.contains("<defaultValue>0.22.0</defaultValue>"));
        assert!(xml.contains("<name>DEEPSWE_REF</name>"));
        assert!(xml.contains(
            "<defaultValue>0b9fabbb63b9104d678fe965e1632f2dd9eaa2ea</defaultValue>"
        ));
        assert!(xml.contains("<name>LOLBENCH_REF</name>"));
        assert!(xml.contains("<name>ICODE_EXPECT_SHA</name>"));
        assert!(xml.contains("<name>OFFICIAL</name>"));
        assert!(xml.contains("<defaultValue>0</defaultValue>"));
        assert!(xml.contains("<name>CANARY</name>"));
        assert!(xml.contains("<string>official</string>"));
        assert!(xml.contains("<string>only</string>"));
        assert!(xml.contains("<name>CANARY_ALLOW_HOST</name>"));
        assert!(xml.contains("OFFICIAL=1 refuses it"));
        assert!(xml.contains("StashedFileParameterDefinition"));
        assert!(
            !xml.contains("<name>ICODE_RELEASE</name>"),
            "Jenkins UI must not ask for a worker ICODE_RELEASE path"
        );
        assert!(xml.contains("<?xml version='1.0'"));
        assert!(xml.contains("leave this control as it is"));
        assert!(xml.contains("Vice versa"));
        assert!(xml.contains("<string>branch</string>"));
        assert!(xml.contains("<string>tag</string>"));
        assert!(xml.contains("<string>commit</string>"));
        assert!(xml.contains("<string>pr</string>"));
        assert!(
            !xml.contains("<string>auto</string>"),
            "Jenkins KIND list is branch/tag/commit/pr only"
        );
        assert!(xml.contains("Full suite is 20"));
        assert!(xml.contains("wins over TASK and N_TASKS"));
        assert!(
            !xml.contains('\u{2013}'),
            "en-dash breaks Jenkins XML 1.1 POST"
        );
        assert!(
            !xml.contains('\u{2192}'),
            "unicode arrow breaks Jenkins XML 1.1 POST"
        );
        assert!(!xml.contains("<name>ICODE_SOURCE</name>"));
        assert!(!xml.contains("<string>source</string>"));
        assert!(!xml.contains("<name>EVAL_MODE</name>"));
        assert!(!xml.contains("<name>LOLBENCH_PATH</name>"));
        assert!(xml.contains("pipeline {"));
        assert!(xml.contains("run_all.sh"));
    }

    #[test]
    fn job_opts_from_config_falls_back_to_old_binary_target() {
        let mut cfg = crate::config::MacK3dConfig::default();
        cfg.jenkins_job.default_icode_release.clear();
        cfg.jenkins_job.default_binary_target = "/tmp/icode".into();
        let opts = JobOpts::from_config(&cfg, Vec::new());
        assert_eq!(opts.default_icode_release, "/tmp/icode");
        assert!(opts.default_task.is_empty());
        assert_eq!(opts.default_eval_mode, "git");
        assert_eq!(opts.default_icode_git_ref, "2");
        assert_eq!(opts.default_icode_git_ref_kind, "pr");
        assert_eq!(opts.default_n_rollouts, 4);
        assert_eq!(
            opts.default_icode_git_url,
            "https://gitcode.com/michaelling/jiuwenicode"
        );
    }

    #[test]
    fn job_opts_from_config_normalizes_eval_mode() {
        let mut cfg = crate::config::MacK3dConfig::default();
        cfg.jenkins_job.default_eval_mode = "SOURCE".into();
        cfg.jenkins_job.default_icode_git_url = "https://gitcode.com/example/icode.git".into();
        cfg.jenkins_job.default_icode_git_ref.clear();
        let opts = JobOpts::from_config(&cfg, Vec::new());
        assert_eq!(opts.default_eval_mode, "git");
        assert_eq!(opts.default_icode_git_ref, "2");
        assert_eq!(
            opts.default_icode_git_url,
            "https://gitcode.com/example/icode.git"
        );
        let jf = jenkinsfile(&opts);
        assert!(jf.contains("ICODE_MODE:-git"));
        assert!(!jf.contains("export ICODE_SOURCE="));
    }

    #[test]
    fn base64_roundtrip_ascii() {
        let s = "hello pipeline";
        let enc = base64_encode(s.as_bytes());
        assert_eq!(enc, "aGVsbG8gcGlwZWxpbmU=");
    }

    #[test]
    fn deepswe_one_task_xml_mentions_progress_and_scripts() {
        let xml = deepswe_one_task_job_xml(&deepswe_opts(vec!["deepseek-api-key".into()]));
        assert!(xml.contains("<![CDATA["));
        assert!(xml.contains("pipeline/stages/run_all.sh"));
        assert!(!xml.contains("Documents/Toby/mac-k3d"));
        assert!(xml.contains("PROGRESS"));
        assert!(xml.contains("deepswe"));
        assert!(xml.contains("deepseek"));
        assert!(xml.contains("DEEPSEEK_MODEL"));
        assert!(xml.contains("deepseek-v4-pro"));
        assert!(xml.contains("withCredentials"));
        assert!(xml.contains("ParametersDefinitionProperty"));
        assert!(xml.contains("<name>N_TASKS</name>"));
        assert!(xml.contains("<name>N_ROLLOUTS</name>"));
        assert!(xml.contains("<name>RESUME</name>"));
        assert!(xml.contains("BooleanParameterDefinition"));
        assert!(xml.contains("<defaultValue>1</defaultValue>"));
        assert!(xml.contains("<name>TASK</name>"));
        // A worker no longer needs a pre-extracted pipeline, so the old "run
        // mac-k3d config on the worker" hints are gone; the clone is the fix.
        assert!(!xml.contains("mac-k3d config -c worker.yaml"));
        assert!(!xml.contains("eval --stage p0"));
        assert!(xml.contains("has no pipeline/stages/run_all.sh"));
        assert!(xml.contains("user-guide"));
        assert!(!xml.contains("harbor run"));
        assert!(
            !xml.contains("/home/Toby/Documents/Toby/iCode-main"),
            "job must not hardcode a lab iCode path"
        );
        assert!(xml.contains("export RESUME="));
        assert!(xml.contains("booleanParam(name: 'RESUME'"));
    }

    #[test]
    fn deepswe_one_task_xml_omits_bind_when_credential_ids_empty() {
        let xml = deepswe_one_task_job_xml(&deepswe_opts(Vec::new()));
        assert!(
            !xml.contains("withCredentials"),
            "empty IDs must omit the bind"
        );
    }

    #[test]
    fn skip_secrets_must_pass_listed_ids_to_keep_bind() {
        // --skip-secrets / start list existing IDs then call this helper; [] would strip the bind.
        let xml = deepswe_one_task_job_xml(&deepswe_opts(vec!["deepseek-api-key".into()]));
        assert!(xml.contains("withCredentials"));
        assert!(xml.contains("deepseek-api-key"));
        assert!(xml.contains("DEEPSEEK_API_KEY"));
    }

    #[test]
    fn both_jobs_share_run_all_and_differ_by_benchmark() {
        let deepswe = deepswe_one_task_job_xml(&deepswe_opts(vec!["deepseek-api-key".into()]));
        let lolbench = job_config_xml(&sample_opts());
        let swebenchpro =
            swebenchpro_one_task_job_xml(&deepswe_opts(vec!["deepseek-api-key".into()]));
        assert!(deepswe.contains("<string>deepswe</string>"));
        assert!(lolbench.contains("<string>lolbench</string>"));
        assert!(swebenchpro.contains("<string>swebenchpro</string>"));
        assert!(deepswe.contains("run_all.sh"));
        assert!(lolbench.contains("run_all.sh"));
        assert!(swebenchpro.contains("run_all.sh"));
        assert!(lolbench.contains("Evaluate the iCode harness with an LLM call on one LoLBench task"));
        assert!(deepswe.contains("Evaluate the iCode harness with an LLM call on one DeepSWE task"));
        assert!(swebenchpro.contains(
            "Evaluate the iCode harness with an LLM call on one SWE-bench Pro task"
        ));
        assert!(lolbench.contains("One LoLBench question id"));
        assert!(deepswe.contains("One DeepSWE question id"));
        assert!(swebenchpro.contains("One SWE-bench Pro instance id"));
        assert!(!lolbench.contains("stays on Pier"));
        assert!(!deepswe.contains("stays on Pier"));
        assert!(!swebenchpro.contains("stays on Pier"));
        assert!(!deepswe.contains("harbor run"));
        assert!(!lolbench.contains("harbor run"));
        assert!(deepswe.contains("StashedFileParameterDefinition"));
        assert!(lolbench.contains("StashedFileParameterDefinition"));
        assert!(swebenchpro.contains("StashedFileParameterDefinition"));
        assert!(!deepswe.contains("<name>ICODE_RELEASE</name>"));
        assert!(!lolbench.contains("<name>ICODE_RELEASE</name>"));
        assert!(!swebenchpro.contains("<name>ICODE_RELEASE</name>"));
    }

    #[test]
    fn question_defaults_apply_to_all_jobs() {
        let mut opts = sample_opts();
        opts.default_benchmark = "deepswe".into();
        opts.default_task = "abs-stepped-slices".into();
        opts.default_n_tasks = 2;
        opts.default_tasks.clear();
        let deepswe = deepswe_one_task_job_xml(&opts);
        let lolbench = job_config_xml(&opts);
        let swebenchpro = swebenchpro_one_task_job_xml(&opts);
        assert!(deepswe.contains("<defaultValue>abs-stepped-slices</defaultValue>"));
        assert!(lolbench.contains("<defaultValue>abs-stepped-slices</defaultValue>"));
        assert!(swebenchpro.contains("<defaultValue>abs-stepped-slices</defaultValue>"));
        assert!(deepswe.contains("<name>N_TASKS</name>"));
        assert!(deepswe.contains("Full suite is 113"));
        assert!(swebenchpro.contains("Full suite is 731"));
        assert!(deepswe.contains("<name>N_ROLLOUTS</name>"));
        // one_task is the smoke shape: one question, one rollout, whatever the
        // config's N_TASKS says. some_task is where the configured count lands.
        assert!(deepswe.contains("<name>N_TASKS</name>\n          <description>Used only when TASK and TASKS are empty"));
        let some = eval_job_xml("deepswe", JobShape::Some_, "d", &opts);
        assert!(some.contains("<defaultValue>2</defaultValue>"));
        let (task, n, tasks) = question_defaults_for_job(&opts, "swebenchpro");
        assert_eq!(task, "abs-stepped-slices");
        assert_eq!(n, 2);
        assert!(tasks.is_empty());
        assert!(swebenchpro.contains("<name>N_TASKS</name>"));
        assert!(swebenchpro.contains("<name>N_ROLLOUTS</name>"));
        assert!(lolbench.contains("<name>N_ROLLOUTS</name>"));
        assert!(lolbench.contains("<name>HARNESS</name>"));
        assert!(lolbench.contains("<string>icode</string>"));
        assert!(lolbench.contains("<name>LLM</name>"));
        assert!(lolbench.contains("<string>deepseek</string>"));
    }

    #[test]
    fn tasks_list_becomes_tasks_param() {
        let mut opts = deepswe_opts(Vec::new());
        opts.default_task.clear();
        opts.default_tasks = vec!["abs-module-cache-flags".into(), "abs-stepped-slices".into()];
        let xml = deepswe_one_task_job_xml(&opts);
        assert!(xml.contains("<name>TASKS</name>"));
        assert!(
            xml.contains("<defaultValue>abs-module-cache-flags,abs-stepped-slices</defaultValue>")
        );
        assert!(xml.contains("export TASKS="));
        let (task, n, tasks) = question_defaults_for_job(&opts, "deepswe");
        assert!(task.is_empty());
        assert_eq!(n, 2);
        assert_eq!(tasks, "abs-module-cache-flags,abs-stepped-slices");
    }

    #[test]
    fn job_xml_file_param_does_not_bake_worker_release_path() {
        let mut opts = sample_opts();
        opts.default_icode_release = "/tmp/lab-icode-full-drop".into();
        let xml = job_config_xml(&opts);
        assert!(xml.contains("<name>ICODE_RELEASE_FILE</name>"));
        assert!(xml.contains("StashedFileParameterDefinition"));
        assert!(!xml.contains("/tmp/lab-icode-full-drop"));
        assert!(!xml.contains("file(name:"));
        assert!(xml.contains("stashedFile(name: 'ICODE_RELEASE_FILE'"));
        assert!(xml.contains("unstash 'ICODE_RELEASE_FILE'"));
    }

    #[test]
    fn deepseek_model_is_catalog_choice_with_both_ids() {
        let xml = deepswe_one_task_job_xml(&deepswe_opts(Vec::new()));
        assert!(xml.contains("<name>DEEPSEEK_MODEL</name>"));
        assert!(xml.contains("<string>deepseek-v4-pro</string>"));
        assert!(xml.contains("<string>deepseek-flash</string>"));
        assert!(xml.contains("ChoiceParameterDefinition"));
        assert!(!xml.contains("<defaultValue>deepseek-v4-pro</defaultValue>"));

        let mut opts = sample_opts();
        opts.default_deepseek_model = "deepseek-flash".into();
        let jf = jenkinsfile(&opts);
        assert!(
            jf.contains(
                "choice(name: 'DEEPSEEK_MODEL', choices: ['deepseek-flash', 'deepseek-v4-pro']"
            ),
            "{jf}"
        );
        assert!(jf.contains("export DEEPSEEK_MODEL=\"${DEEPSEEK_MODEL:-deepseek-v4-pro}\""));
    }

    #[test]
    fn job_opts_empty_deepseek_model_uses_catalog_default() {
        let cfg = crate::config::MacK3dConfig::default();
        let opts = JobOpts::from_config(&cfg, Vec::new());
        assert_eq!(opts.default_deepseek_model, eval_catalog::default_model());
    }

    #[test]
    fn dump_swebenchpro_job_xml_when_env_set() {
        let Ok(path) = std::env::var("DUMP_SWEBENCHPRO_JOB_XML") else {
            return;
        };
        if path.trim().is_empty() {
            return;
        }
        let mut opts = sample_opts();
        opts.default_icode_git_url = String::new();
        opts.default_benchmark = "swebenchpro".into();
        opts.default_task = String::new();
        opts.default_n_tasks = 1;
        opts.default_deepseek_model = "deepseek-flash".into();
        std::fs::write(&path, swebenchpro_one_task_job_xml(&opts)).expect("write xml");
        if let Ok(dir) = std::env::var("DUMP_ALL_ONE_TASK_XML_DIR") {
            if !dir.trim().is_empty() {
                let _ = std::fs::create_dir_all(&dir);
                let mut lol = sample_opts();
                lol.default_icode_git_url = String::new();
                lol.default_deepseek_model = "deepseek-flash".into();
                std::fs::write(
                    format!("{dir}/lolbench_one_task.xml"),
                    job_config_xml(&lol),
                )
                .expect("write lolbench xml");
                let mut ds = deepswe_opts(vec![
                    "deepseek-api-key".into(),
                    "gitcode-pat".into(),
                ]);
                ds.default_deepseek_model = "deepseek-flash".into();
                std::fs::write(
                    format!("{dir}/deepswe_one_task.xml"),
                    deepswe_one_task_job_xml(&ds),
                )
                .expect("write deepswe xml");
            }
        }
    }
}
