use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::error::{Error, Result};

pub const LOLBENCH_ONE_TASK: &str = "lolbench_one_task";

/// Options used when rendering / updating `lolbench_one_task`.
#[derive(Debug, Clone)]
pub struct JobOpts {
    pub default_task: String,
    pub default_eval_mode: String,
    pub default_icode_release: String,
    pub default_icode_git_url: String,
    pub default_icode_git_ref: String,
    pub default_icode_args: String,
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
        Self {
            default_task: if config.jenkins_job.default_task.trim().is_empty() {
                "ruff_1".into()
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
            credential_ids,
        }
    }
}

fn normalize_eval_mode(s: &str) -> String {
    if s.trim().eq_ignore_ascii_case("source") {
        "source".into()
    } else {
        "binary".into()
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
    if update_job_config_xml(base, auth, crumb, cookie_file, opts).is_ok() {
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
    if default_mode == "source" {
        ("source", "binary")
    } else {
        ("binary", "source")
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
        &opts.default_task,
        "iCode vs DeepSeek baseline on one LoLBench task (Harbor + iCode + deepseek-v4-pro). DeepSWE stays on Pier. See docs/lolbench-jenkins.md and docs/binary-initializer/user-guide.md.",
        &opts.credential_ids,
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn one_task_jenkinsfile(benchmark: &str, default_task: &str, credential_ids: &[String]) -> String {
    let (cred_open, cred_close) = with_credentials_block(credential_ids);
    let bench = groovy_escape(benchmark);
    let task = groovy_escape(default_task);
    format!(
        r#"pipeline {{
  agent {{ label params.AGENT_LABEL }}

  options {{
    timeout(time: 12, unit: 'HOURS')
  }}

  parameters {{
    choice(name: 'HARNESS', choices: ['icode'], description: 'v1: icode only')
    choice(name: 'LLM', choices: ['deepseek'], description: 'v1: deepseek only')
    choice(name: 'BENCHMARK', choices: ['{bench}'], description: 'This job is one benchmark')
    string(name: 'TASK', defaultValue: '{task}', description: 'One question id (empty DeepSWE = first alphabetical). LoLBench example: ruff_1')
    string(name: 'N_TASKS', defaultValue: '1', description: 'Kept at 1 for *_one_task jobs')
    choice(name: 'ICODE_MODE', choices: ['binary', 'source'], description: 'iCode delivery (users: binary drop)')
    string(name: 'ICODE_RELEASE', defaultValue: '', description: 'binary: empty = ~/.local/share/mac-k3d/icode or icode-*-full-* (tar.gz / folder)')
    string(name: 'ICODE_SOURCE', defaultValue: '', description: 'source: path on worker; empty = discover (developers)')
    string(name: 'AGENT_LABEL', defaultValue: 'lolbench')
    string(name: 'CPU_LOCK_QTY', defaultValue: '4')
    string(name: 'MAC_K3D_ROOT', defaultValue: '', description: 'Dir with pipeline/stages/run_all.sh (optional)')
    string(name: 'DEEPSEEK_MODEL', defaultValue: 'deepseek-v4-pro', description: 'DeepSeek Chat Completions model id')
  }}

  stages {{
    stage('Prepare') {{
      steps {{
        lock(label: 'CPU_CORES', quantity: params.CPU_LOCK_QTY as Integer, resource: null) {{
{cred_open}          sh '''
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
            export ICODE_MODE="${{ICODE_MODE:-binary}}"
            export ICODE_RELEASE="${{ICODE_RELEASE:-}}"
            export ICODE_SOURCE="${{ICODE_SOURCE:-}}"
            export HARNESS=icode
            export LLM=deepseek
            export BENCHMARK="${{BENCHMARK:-{bench}}}"
            export DEEPSEEK_MODEL="${{DEEPSEEK_MODEL:-deepseek-v4-pro}}"
            export LLM_NAME="${{LLM_NAME:-DeepSeek V4 Pro}}"
            if [ -z "${{DEEPSEEK_API_KEY:-}}" ]; then
              echo "DEEPSEEK_API_KEY missing. Store credential deepseek-api-key on the Jenkins controller." >&2
              exit 1
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
        cred_open = cred_open,
        cred_close = cred_close,
    )
}

fn one_task_job_xml(
    benchmark: &str,
    default_task: &str,
    description: &str,
    credential_ids: &[String],
) -> String {
    let script = one_task_jenkinsfile(benchmark, default_task, credential_ids);
    let bench_xml = xml_escape(benchmark);
    let task_xml = xml_escape(default_task);
    let desc_xml = xml_escape(description);
    format!(
        r#"<?xml version='1.1' encoding='UTF-8'?>
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
              <string>icode</string>
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
        <hudson.model.ChoiceParameterDefinition>
          <name>LLM</name>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
              <string>deepseek</string>
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
          <name>N_TASKS</name>
          <defaultValue>1</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.ChoiceParameterDefinition>
          <name>ICODE_MODE</name>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
              <string>binary</string>
              <string>source</string>
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>ICODE_RELEASE</name>
          <defaultValue></defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>ICODE_SOURCE</name>
          <defaultValue></defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
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
        <hudson.model.StringParameterDefinition>
          <name>DEEPSEEK_MODEL</name>
          <defaultValue>deepseek-v4-pro</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
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
    one_task_jenkinsfile("lolbench", &opts.default_task, &opts.credential_ids)
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
    credential_ids: &[String],
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

    let xml = deepswe_one_task_job_xml(credential_ids);

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
            println!("Skipping '{DEEPSWE_ONE_TASK}' create — could not read Jenkins admin password yet.");
            return Ok(());
        }
    };
    ensure_deepswe_one_task(&url, "admin", &password, &credential_ids)
}

fn deepswe_one_task_job_xml(credential_ids: &[String]) -> String {
    one_task_job_xml(
        "deepswe",
        "",
        "iCode vs DeepSeek baseline on one DeepSWE task (Pier + iCode + deepseek-v4-pro). Arm A = iCode + LLM; Arm B = the same LLM without iCode. See docs/binary-initializer/user-guide.md.",
        credential_ids,
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
            credential_ids: vec!["deepseek-api-key".into(), "gitcode-pat".into()],
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
        assert!(jf.contains("HARNESS=icode"));
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
        assert!(xml.contains("<string>binary</string>"));
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
        assert_eq!(opts.default_eval_mode, "binary");
        assert_eq!(opts.default_icode_git_ref, "main");
    }

    #[test]
    fn job_opts_from_config_normalizes_eval_mode() {
        let mut cfg = crate::config::MacK3dConfig::default();
        cfg.jenkins_job.default_eval_mode = "SOURCE".into();
        cfg.jenkins_job.default_icode_git_url = "https://gitcode.com/example/icode.git".into();
        cfg.jenkins_job.default_icode_git_ref.clear();
        let opts = JobOpts::from_config(&cfg, Vec::new());
        assert_eq!(opts.default_eval_mode, "source");
        assert_eq!(opts.default_icode_git_ref, "main");
        assert_eq!(
            opts.default_icode_git_url,
            "https://gitcode.com/example/icode.git"
        );
    }

    #[test]
    fn base64_roundtrip_ascii() {
        let s = "hello pipeline";
        let enc = base64_encode(s.as_bytes());
        assert_eq!(enc, "aGVsbG8gcGlwZWxpbmU=");
    }

    #[test]
    fn deepswe_one_task_xml_mentions_progress_and_scripts() {
        let xml = deepswe_one_task_job_xml(&["deepseek-api-key".into()]);
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
        let xml = deepswe_one_task_job_xml(&[]);
        assert!(
            !xml.contains("withCredentials"),
            "empty IDs must omit the bind"
        );
    }

    #[test]
    fn skip_secrets_must_pass_listed_ids_to_keep_bind() {
        // --skip-secrets / start list existing IDs then call this helper; [] would strip the bind.
        let xml = deepswe_one_task_job_xml(&["deepseek-api-key".into()]);
        assert!(xml.contains("withCredentials"));
        assert!(xml.contains("deepseek-api-key"));
        assert!(xml.contains("DEEPSEEK_API_KEY"));
    }

    #[test]
    fn both_jobs_share_run_all_and_differ_by_benchmark() {
        let deepswe = deepswe_one_task_job_xml(&["deepseek-api-key".into()]);
        let lolbench = job_config_xml(&sample_opts());
        assert!(deepswe.contains("<string>deepswe</string>"));
        assert!(lolbench.contains("<string>lolbench</string>"));
        assert!(deepswe.contains("run_all.sh"));
        assert!(lolbench.contains("run_all.sh"));
        assert!(lolbench.contains("Harbor + iCode"));
        assert!(deepswe.contains("Pier + iCode"));
        assert!(!deepswe.contains("harbor run"));
        assert!(!lolbench.contains("harbor run"));
    }
}
