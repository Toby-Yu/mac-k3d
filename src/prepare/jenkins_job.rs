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
const DESC_ICODE_GIT_REF: &str = "Git: branch name, tag, or commit SHA (match KIND). Release: do not change this; leave it as it is. Vice versa: if you chose release, ignore this; if you chose git, this is required.";
const DESC_ICODE_GIT_REF_KIND: &str = "Git: pick branch, tag, or commit (no auto). Release: do not change this; leave it as it is. Vice versa: if you chose release, ignore this; if you chose git, pick the kind that matches REF.";

/// Options used when rendering / updating `lolbench_one_task`.
#[derive(Debug, Clone)]
pub struct JobOpts {
    pub default_task: String,
    pub default_eval_mode: String,
    pub default_icode_release: String,
    pub default_icode_git_url: String,
    pub default_icode_git_ref: String,
    pub default_icode_args: String,
    pub default_harness: String,
    pub default_llm: String,
    pub default_deepseek_model: String,
    pub default_benchmark: String,
    pub default_n_tasks: u32,
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
                "main".into()
            } else {
                git_ref.to_string()
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
    one_task_job_xml(
        "lolbench",
        "iCode vs DeepSeek baseline on one LoLBench task (Harbor + iCode + DeepSeek catalog model). DeepSWE stays on Pier. See docs/lolbench-jenkins.md and docs/user-guide.md.",
        opts,
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn job_applies_question_defaults(opts: &JobOpts, job_benchmark: &str) -> bool {
    let b = opts.default_benchmark.trim();
    if b.eq_ignore_ascii_case(job_benchmark) {
        return true;
    }
    b.is_empty() && job_benchmark == "lolbench"
}

fn question_defaults_for_job(opts: &JobOpts, job_benchmark: &str) -> (String, u32, String) {
    if job_applies_question_defaults(opts, job_benchmark) {
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
        return (task, n, tasks);
    }
    if job_benchmark == "lolbench" {
        ("ruff_1".into(), 1, String::new())
    } else {
        (String::new(), 1, String::new())
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
    let (cred_open, cred_close) = with_credentials_block(&opts.credential_ids);
    let bench = groovy_escape(job_benchmark);
    let (task, n_tasks, tasks) = question_defaults_for_job(opts, job_benchmark);
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
    let harness_fb = eval_catalog::HARNESSES[0];
    let llm_fb = eval_catalog::LLMS[0];
    let model_fb = eval_catalog::default_model();
    let icode_mode_choices = eval_catalog::choices_preferred_first(
        eval_catalog::ICODE_CI_MODES,
        jenkins_icode_mode(opts),
    );
    let icode_mode_g = groovy_quoted_list(&icode_mode_choices);
    let icode_mode_fb = jenkins_icode_mode(opts);
    let icode_git_url_g = groovy_escape(opts.default_icode_git_url.trim());
    let icode_git_ref_g = groovy_escape(&opts.default_icode_git_ref);
    let icode_git_ref_kind_choices =
        eval_catalog::choices_preferred_first(eval_catalog::ICODE_GIT_REF_KINDS, "branch");
    let icode_git_ref_kind_g = groovy_quoted_list(&icode_git_ref_kind_choices);
    let icode_mode_desc_g = groovy_escape(DESC_ICODE_MODE);
    let icode_release_file_desc_g = groovy_escape(DESC_ICODE_RELEASE_FILE);
    let icode_git_url_desc_g = groovy_escape(DESC_ICODE_GIT_URL);
    let icode_git_ref_desc_g = groovy_escape(DESC_ICODE_GIT_REF);
    let icode_git_ref_kind_desc_g = groovy_escape(DESC_ICODE_GIT_REF_KIND);
    format!(
        r#"pipeline {{
  agent {{ label params.AGENT_LABEL }}

  options {{
    timeout(time: 12, unit: 'HOURS')
  }}

  parameters {{
    choice(name: 'HARNESS', choices: [{harness_g}], description: 'Eval harness (catalog; v1: icode)')
    choice(name: 'LLM', choices: [{llm_g}], description: 'LLM family (catalog; v1: deepseek)')
    choice(name: 'BENCHMARK', choices: ['{bench}'], description: 'This job is one benchmark')
    string(name: 'TASK', defaultValue: '{task}', description: 'One question id (empty + N_TASKS = first N sorted). LoLBench example: ruff_1')
    string(name: 'TASKS', defaultValue: '{tasks}', description: 'Comma-separated question ids (overrides TASK and first-N)')
    string(name: 'N_TASKS', defaultValue: '{n_tasks}', description: 'First N sorted questions when TASK and TASKS are empty. N>1 is slower/costlier.')
    choice(name: 'ICODE_MODE', choices: [{icode_mode_g}], description: '{icode_mode_desc_g}')
    stashedFile(name: 'ICODE_RELEASE_FILE', description: '{icode_release_file_desc_g}')
    string(name: 'ICODE_GIT_URL', defaultValue: '{icode_git_url_g}', description: '{icode_git_url_desc_g}')
    string(name: 'ICODE_GIT_REF', defaultValue: '{icode_git_ref_g}', description: '{icode_git_ref_desc_g}')
    choice(name: 'ICODE_GIT_REF_KIND', choices: [{icode_git_ref_kind_g}], description: '{icode_git_ref_kind_desc_g}')
    string(name: 'AGENT_LABEL', defaultValue: 'lolbench')
    string(name: 'CPU_LOCK_QTY', defaultValue: '4')
    string(name: 'MAC_K3D_ROOT', defaultValue: '', description: 'Dir with pipeline/stages/run_all.sh (optional)')
    choice(name: 'DEEPSEEK_MODEL', choices: [{model_g}], description: 'DeepSeek Chat Completions model id (catalog)')
  }}

  stages {{
    stage('Prepare') {{
      steps {{
        lock(label: 'CPU_CORES', quantity: params.CPU_LOCK_QTY as Integer, resource: null) {{
{cred_open}          script {{
            try {{
              unstash 'ICODE_RELEASE_FILE'
            }} catch (Throwable t) {{
              echo "No ICODE_RELEASE_FILE stash (${{t}})"
            }}
          }}
          sh '''
            set -euo pipefail
            echo "PROGRESS 5% prepare workspace"
            export PATH="${{HOME}}/.local/bin:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:${{PATH}}"
            command -v docker >/dev/null
            docker info >/dev/null
            SHARE="${{XDG_DATA_HOME:-$HOME/.local/share}}/mac-k3d"
            ROOT="${{MAC_K3D_ROOT:-}}"
            if [ -z "$ROOT" ] || [ ! -f "$ROOT/pipeline/stages/run_all.sh" ]; then
              if [ -f "${{WORKSPACE}}/pipeline/stages/run_all.sh" ]; then
                ROOT="${{WORKSPACE}}"
              elif [ -f "$SHARE/pipeline/stages/run_all.sh" ]; then
                ROOT="$SHARE"
              else
                echo "MAC_K3D_ROOT missing pipeline/stages/run_all.sh. On this worker run: mac-k3d config -c worker.yaml (extracts ~/.local/share/mac-k3d/pipeline) or: mac-k3d eval --stage p0" >&2
                exit 1
              fi
            fi
            export MAC_K3D_ROOT="$ROOT"
            export MAC_K3D_EVAL_WORKDIR="${{WORKSPACE}}/eval-runs"
            export N_TASKS="${{N_TASKS:-1}}"
            export TASK="${{TASK:-}}"
            export TASKS="${{TASKS:-}}"
            export ICODE_MODE="${{ICODE_MODE:-{icode_mode_fb}}}"
            export ICODE_RELEASE=""
            export ICODE_RELEASE_UPLOADED=""
            if [ "${{ICODE_MODE}}" = "git" ]; then
              bash "$MAC_K3D_ROOT/pipeline/lib/icode_input.sh" --force-rm "${{WORKSPACE}}/ICODE_RELEASE_FILE"
              export ICODE_RELEASE=""
              export ICODE_RELEASE_UPLOADED=""
            elif [ -s "${{WORKSPACE}}/ICODE_RELEASE_FILE" ]; then
              export ICODE_RELEASE="${{WORKSPACE}}/ICODE_RELEASE_FILE"
              export ICODE_RELEASE_UPLOADED=1
            fi
            export ICODE_GIT_URL="${{ICODE_GIT_URL:-}}"
            export ICODE_GIT_REF="${{ICODE_GIT_REF:-main}}"
            export ICODE_GIT_REF_KIND="${{ICODE_GIT_REF_KIND:-branch}}"
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
            echo "PROGRESS 10% running pipeline/stages"
            bash "$MAC_K3D_ROOT/pipeline/stages/run_all.sh"
            echo "PROGRESS 100% done"
            if [ -f "$MAC_K3D_EVAL_WORKDIR/last_output.txt" ]; then
              echo "RESULT $(cat "$MAC_K3D_EVAL_WORKDIR/last_output.txt")"
            fi
          '''
{cred_close}        }}
      }}
    }}
  }}

  post {{
    always {{
      archiveArtifacts artifacts: 'eval-runs/reports/**/*.json', allowEmptyArchive: true
    }}
  }}
}}
"#,
        bench = bench,
        task = task,
        tasks = tasks,
        n_tasks = n_tasks,
        harness_g = harness_g,
        llm_g = llm_g,
        model_g = model_g,
        harness_fb = harness_fb,
        llm_fb = llm_fb,
        model_fb = model_fb,
        cred_open = cred_open,
        cred_close = cred_close,
        icode_mode_g = icode_mode_g,
        icode_mode_fb = icode_mode_fb,
        icode_git_url_g = icode_git_url_g,
        icode_git_ref_g = icode_git_ref_g,
        icode_git_ref_kind_g = icode_git_ref_kind_g,
        icode_mode_desc_g = icode_mode_desc_g,
        icode_release_file_desc_g = icode_release_file_desc_g,
        icode_git_url_desc_g = icode_git_url_desc_g,
        icode_git_ref_desc_g = icode_git_ref_desc_g,
        icode_git_ref_kind_desc_g = icode_git_ref_kind_desc_g,
    )
}

fn one_task_job_xml(job_benchmark: &str, description: &str, opts: &JobOpts) -> String {
    let script = one_task_jenkinsfile(job_benchmark, opts);
    let (task, n_tasks, tasks) = question_defaults_for_job(opts, job_benchmark);
    let bench_xml = xml_escape(job_benchmark);
    let task_xml = xml_escape(&task);
    let tasks_xml = xml_escape(&tasks);
    let desc_xml = xml_escape(description);
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
    let icode_git_ref_kind_choices =
        eval_catalog::choices_preferred_first(eval_catalog::ICODE_GIT_REF_KINDS, "branch");
    let icode_git_kind_xml = xml_choice_strings(&icode_git_ref_kind_choices);
    let icode_mode_desc_xml = xml_escape(DESC_ICODE_MODE);
    let icode_release_file_desc_xml = xml_escape(DESC_ICODE_RELEASE_FILE);
    let icode_git_url_desc_xml = xml_escape(DESC_ICODE_GIT_URL);
    let icode_git_ref_desc_xml = xml_escape(DESC_ICODE_GIT_REF);
    let icode_git_ref_kind_desc_xml = xml_escape(DESC_ICODE_GIT_REF_KIND);
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
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
              <string>{bench_xml}</string>
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>TASK</name>
          <defaultValue>{task_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>TASKS</name>
          <defaultValue>{tasks_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>N_TASKS</name>
          <defaultValue>{n_tasks}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
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
          <name>AGENT_LABEL</name>
          <defaultValue>lolbench</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>CPU_LOCK_QTY</name>
          <defaultValue>4</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>MAC_K3D_ROOT</name>
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

/// Ensure Pipeline job `deepswe_one_task` (Pier + DeepSWE + iCode vs DeepSeek baseline).
pub fn ensure_deepswe_one_task(
    jenkins_url: &str,
    api_user: &str,
    api_token_or_password: &str,
    opts: &JobOpts,
) -> Result<()> {
    let base = jenkins_url.trim_end_matches('/');
    let auth = format!("{api_user}:{api_token_or_password}");

    wait_for_jenkins(base, &auth, Duration::from_secs(90))?;

    let cookie_file = tempfile_path("mac-k3d-deepswe-one-task-cookies")?;
    let crumb = fetch_crumb(base, &auth, &cookie_file);

    let exists = curl_status(
        base,
        &auth,
        &format!("/job/{DEEPSWE_ONE_TASK}/api/json"),
        &crumb,
        &cookie_file,
    )
    .map(|c| c == 200)
    .unwrap_or(false);

    let xml = deepswe_one_task_job_xml(opts);

    if exists {
        println!("Updating Jenkins job '{DEEPSWE_ONE_TASK}'…");
        let post_url = format!("{base}/job/{DEEPSWE_ONE_TASK}/config.xml");
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
            &xml,
            "-w",
            "\n%{http_code}",
        ]);
        if let Some((field, value)) = &crumb {
            cmd.args(["-H", &format!("{field}: {value}")]);
        }
        let output = cmd.output().map_err(|e| Error::CommandFailed {
            cmd: "curl config.xml deepswe_one_task".into(),
            source: e.into(),
        })?;
        let _ = std::fs::remove_file(&cookie_file);
        let raw = String::from_utf8_lossy(&output.stdout);
        let code = raw.lines().last().unwrap_or("").trim().to_string();
        if code != "200" && code != "201" {
            println!("Warning: failed to update '{DEEPSWE_ONE_TASK}' (HTTP {code}).");
        } else {
            println!("Updated job '{DEEPSWE_ONE_TASK}'.");
        }
        return Ok(());
    }

    let create_url = format!(
        "{base}/createItem?name={}",
        urlencoding_simple(DEEPSWE_ONE_TASK)
    );
    println!("Creating Jenkins job '{DEEPSWE_ONE_TASK}' on {base} …");
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
        cmd: "curl createItem deepswe_one_task".into(),
        source: e.into(),
    })?;
    let _ = std::fs::remove_file(&cookie_file);
    let raw = String::from_utf8_lossy(&output.stdout);
    let code = raw.lines().last().unwrap_or("").trim().to_string();
    if code != "200" && code != "201" && code != "302" && code != "303" {
        println!("Warning: failed to create '{DEEPSWE_ONE_TASK}' (HTTP {code}).");
        return Ok(());
    }
    println!(
        "Created job '{DEEPSWE_ONE_TASK}'.\n\
         Trigger: {base}/job/{DEEPSWE_ONE_TASK}/buildWithParameters  (or: mac-k3d eval)"
    );
    Ok(())
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

fn deepswe_one_task_job_xml(opts: &JobOpts) -> String {
    one_task_job_xml(
        "deepswe",
        "iCode vs DeepSeek baseline on one DeepSWE task (Pier + iCode + DeepSeek catalog model). Arm A = iCode + LLM; Arm B = the same LLM without iCode. See docs/user-guide.md.",
        opts,
    )
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
            default_icode_args: "--help".into(),
            default_harness: "icode".into(),
            default_llm: "deepseek".into(),
            default_deepseek_model: eval_catalog::default_model().into(),
            default_benchmark: "lolbench".into(),
            default_n_tasks: 1,
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
            default_icode_args: String::new(),
            default_harness: "icode".into(),
            default_llm: "deepseek".into(),
            default_deepseek_model: eval_catalog::default_model().into(),
            default_benchmark: "deepswe".into(),
            default_n_tasks: 1,
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
        assert!(jf.contains("stashedFile(name: 'ICODE_RELEASE_FILE'"));
        assert!(jf.contains("unstash 'ICODE_RELEASE_FILE'"));
        assert!(jf.contains("--force-rm"));
        assert!(jf.contains(r#"if [ "${ICODE_MODE}" = "git" ]"#));
        assert!(jf.contains("leave this control as it is"));
        assert!(jf.contains("Vice versa"));
        assert!(jf.contains("ICODE_GIT_REF_KIND:-branch"));
        assert!(!jf.contains("ICODE_GIT_REF_KIND:-auto"));
        assert!(jf.contains("upload ICODE_RELEASE_FILE"));
        assert!(jf.contains("icode"));
        assert!(jf.contains("lock(label: 'CPU_CORES'"));
        assert!(jf.contains("withCredentials"));
        assert!(jf.contains("deepseek-api-key"));
        assert!(jf.contains("deepseek-v4-pro"));
        assert!(!jf.contains("harbor run"));
        assert!(!jf.contains("EVAL_MODE"));
        assert!(!jf.contains("icode-in"));
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
        assert!(
            !xml.contains("<string>auto</string>"),
            "Jenkins KIND list is branch/tag/commit only"
        );
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
        assert_eq!(opts.default_task, "ruff_1");
        assert_eq!(opts.default_eval_mode, "release");
        assert_eq!(opts.default_icode_git_ref, "main");
    }

    #[test]
    fn job_opts_from_config_normalizes_eval_mode() {
        let mut cfg = crate::config::MacK3dConfig::default();
        cfg.jenkins_job.default_eval_mode = "SOURCE".into();
        cfg.jenkins_job.default_icode_git_url = "https://gitcode.com/example/icode.git".into();
        cfg.jenkins_job.default_icode_git_ref.clear();
        let opts = JobOpts::from_config(&cfg, Vec::new());
        assert_eq!(opts.default_eval_mode, "git");
        assert_eq!(opts.default_icode_git_ref, "main");
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
        assert!(xml.contains(".local/share"));
        assert!(!xml.contains("Documents/Toby/mac-k3d"));
        assert!(xml.contains("PROGRESS"));
        assert!(xml.contains("deepswe"));
        assert!(xml.contains("deepseek"));
        assert!(xml.contains("DEEPSEEK_MODEL"));
        assert!(xml.contains("deepseek-v4-pro"));
        assert!(xml.contains("withCredentials"));
        assert!(xml.contains("ParametersDefinitionProperty"));
        assert!(xml.contains("<name>N_TASKS</name>"));
        assert!(xml.contains("<name>TASK</name>"));
        assert!(xml.contains("mac-k3d config -c worker.yaml"));
        assert!(xml.contains("eval --stage p0"));
        assert!(xml.contains("user-guide"));
        assert!(!xml.contains("harbor run"));
        assert!(
            !xml.contains("/home/Toby/Documents/Toby/iCode-main"),
            "job must not hardcode a lab iCode path"
        );
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
        assert!(deepswe.contains("<string>deepswe</string>"));
        assert!(lolbench.contains("<string>lolbench</string>"));
        assert!(deepswe.contains("run_all.sh"));
        assert!(lolbench.contains("run_all.sh"));
        assert!(lolbench.contains("Harbor + iCode"));
        assert!(deepswe.contains("Pier + iCode"));
        assert!(!deepswe.contains("harbor run"));
        assert!(!lolbench.contains("harbor run"));
        assert!(deepswe.contains("StashedFileParameterDefinition"));
        assert!(lolbench.contains("StashedFileParameterDefinition"));
        assert!(!deepswe.contains("<name>ICODE_RELEASE</name>"));
        assert!(!lolbench.contains("<name>ICODE_RELEASE</name>"));
    }

    #[test]
    fn question_defaults_apply_to_matching_job_only() {
        let mut opts = sample_opts();
        opts.default_benchmark = "deepswe".into();
        opts.default_task = "abs-stepped-slices".into();
        opts.default_n_tasks = 2;
        opts.default_tasks.clear();
        let deepswe = deepswe_one_task_job_xml(&opts);
        let lolbench = job_config_xml(&opts);
        assert!(deepswe.contains("<defaultValue>abs-stepped-slices</defaultValue>"));
        assert!(deepswe.contains("<name>N_TASKS</name>"));
        assert!(deepswe.contains("<defaultValue>2</defaultValue>"));
        assert!(lolbench.contains("<defaultValue>ruff_1</defaultValue>"));
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
}
