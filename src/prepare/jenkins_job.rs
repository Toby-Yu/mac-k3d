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
    let script = jenkinsfile(opts);
    let task_xml = xml_escape(&opts.default_task);
    let release_xml = xml_escape(&opts.default_icode_release);
    let git_url_xml = xml_escape(&opts.default_icode_git_url);
    let git_ref_xml = xml_escape(&opts.default_icode_git_ref);
    let args_xml = xml_escape(&opts.default_icode_args);
    let (mode_first, mode_second) = eval_mode_choices(&opts.default_eval_mode);

    format!(
        r#"<?xml version='1.1' encoding='UTF-8'?>
<flow-definition plugin="workflow-job">
  <description>iCode eval: EVAL_MODE=binary (GitCode -full- tarball) or source (git + uv). See docs/lolbench-jenkins.md.</description>
  <keepDependencies>false</keepDependencies>
  <properties>
    <hudson.model.ParametersDefinitionProperty>
      <parameterDefinitions>
        <hudson.model.ChoiceParameterDefinition>
          <name>EVAL_MODE</name>
          <description>binary: GitCode -full- tarball; source: git clone + uv sync in the job workspace</description>
          <choices class="java.util.Arrays$ArrayList">
            <a class="string-array">
              <string>{mode_first}</string>
              <string>{mode_second}</string>
            </a>
          </choices>
        </hudson.model.ChoiceParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>TASK</name>
          <description>Case id exported as TASK and ICODE_TASK (not a Harbor path)</description>
          <defaultValue>{task_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>ICODE_RELEASE</name>
          <description>binary mode: icode-OS-ARCH-full-vX.Y.Z.tar.gz URL or path, or a stub icode executable</description>
          <defaultValue>{release_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>ICODE_GIT_URL</name>
          <description>source mode: iCode git URL (private clone uses gitcode-pat)</description>
          <defaultValue>{git_url_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>ICODE_GIT_REF</name>
          <description>source mode: branch or tag</description>
          <defaultValue>{git_ref_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>ICODE_ARGS</name>
          <description>Extra argv after ./icode (smoke: --help)</description>
          <defaultValue>{args_xml}</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>AGENT_LABEL</name>
          <defaultValue>lolbench</defaultValue>
          <trim>true</trim>
        </hudson.model.StringParameterDefinition>
        <hudson.model.StringParameterDefinition>
          <name>CPU_LOCK_QTY</name>
          <description>CPU_CORES lock quantity</description>
          <defaultValue>4</defaultValue>
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

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn jenkinsfile(opts: &JobOpts) -> String {
    let task = groovy_escape(&opts.default_task);
    let release = groovy_escape(&opts.default_icode_release);
    let git_url = groovy_escape(&opts.default_icode_git_url);
    let git_ref = groovy_escape(&opts.default_icode_git_ref);
    let icode_args = groovy_escape(&opts.default_icode_args);
    let (mode_first, mode_second) = eval_mode_choices(&opts.default_eval_mode);
    let (cred_open, cred_close) = with_credentials_block(&opts.credential_ids);

    format!(
        r#"pipeline {{
  agent {{ label params.AGENT_LABEL }}

  options {{
    timeout(time: 7, unit: 'HOURS')
  }}

  parameters {{
    choice(name: 'EVAL_MODE', choices: ['{mode_first}', '{mode_second}'], description: 'binary: GitCode -full- tarball; source: git + uv sync')
    string(name: 'TASK', defaultValue: '{task}', description: 'Case id exported as TASK and ICODE_TASK')
    string(name: 'ICODE_RELEASE', defaultValue: '{release}', description: 'binary: icode-OS-ARCH-full-vX.Y.Z.tar.gz URL or path, or a stub icode')
    string(name: 'ICODE_GIT_URL', defaultValue: '{git_url}', description: 'source: iCode git URL')
    string(name: 'ICODE_GIT_REF', defaultValue: '{git_ref}', description: 'source: branch or tag')
    string(name: 'ICODE_ARGS', defaultValue: '{icode_args}', description: 'Extra argv after ./icode (smoke: --help)')
    string(name: 'AGENT_LABEL', defaultValue: 'lolbench')
    string(name: 'CPU_LOCK_QTY', defaultValue: '4', description: 'CPU_CORES lock quantity')
  }}

  stages {{
    stage('Evaluate') {{
      steps {{
        lock(label: 'CPU_CORES', quantity: params.CPU_LOCK_QTY as Integer, resource: null) {{
{cred_open}          sh '''
            set -euo pipefail
            export PATH="${{HOME}}/.local/bin:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:${{PATH}}"
            command -v docker >/dev/null
            docker info >/dev/null

            work="${{WORKSPACE}}/icode-in"
            rm -rf "$work"
            mkdir -p "$work"
            icode=""
            mode="${{EVAL_MODE:-binary}}"

            run_icode() {{
              if [ -z "$icode" ] || [ ! -e "$icode" ]; then
                echo "could not find icode executable" >&2
                exit 1
              fi
              chmod +x "$icode" || true
              export TASK
              export ICODE_TASK="${{TASK:-}}"
              # iCode README documents ./icode --help and ./icode tui; extra flags via ICODE_ARGS.
              # shellcheck disable=SC2086
              "$icode" ${{ICODE_ARGS:-}}
            }}

            case "$mode" in
              source)
                if [ -z "${{ICODE_GIT_URL:-}}" ]; then
                  echo "ICODE_GIT_URL is required when EVAL_MODE=source" >&2
                  exit 1
                fi
                srcdir="$work/src"
                ref="${{ICODE_GIT_REF:-main}}"
                clone_url="${{ICODE_GIT_URL}}"
                if [ -n "${{GITCODE_TOKEN:-}}" ]; then
                  case "$clone_url" in
                    https://*)
                      rest="${{clone_url#https://}}"
                      clone_url="https://oauth2:${{GITCODE_TOKEN}}@${{rest}}"
                      ;;
                  esac
                fi
                git clone --depth 1 --branch "$ref" "$clone_url" "$srcdir"
                git -C "$srcdir" remote set-url origin "${{ICODE_GIT_URL}}"
                if ! command -v uv >/dev/null; then
                  curl -LsSf https://astral.sh/uv/install.sh | sh
                  export PATH="${{HOME}}/.local/bin:${{PATH}}"
                fi
                command -v uv >/dev/null
                ( cd "$srcdir" && uv sync )
                if [ -x "$srcdir/.venv/bin/icode" ] || [ -f "$srcdir/.venv/bin/icode" ]; then
                  icode="$srcdir/.venv/bin/icode"
                elif [ -x "$srcdir/icode" ] || [ -f "$srcdir/icode" ]; then
                  icode="$srcdir/icode"
                else
                  icode="$(find "$srcdir" -type f -name icode | head -n 1)"
                fi
                run_icode
                ;;
              *)
                if [ -z "${{ICODE_RELEASE:-}}" ]; then
                  echo "ICODE_RELEASE is required (full .tar.gz URL/path, or path to icode)" >&2
                  exit 1
                fi

                src="${{ICODE_RELEASE}}"
                case "$src" in
                  http://*|https://*)
                    if [ -n "${{GITCODE_TOKEN:-}}" ]; then
                      curl -fsSL -H "PRIVATE-TOKEN: ${{GITCODE_TOKEN}}" "$src" -o "$work/release.bin"
                    else
                      curl -fsSL "$src" -o "$work/release.bin"
                    fi
                    src="$work/release.bin"
                    ;;
                esac

                if [ ! -e "$src" ]; then
                  echo "ICODE_RELEASE not found: $src" >&2
                  exit 1
                fi

                base="$(basename "$src")"
                case "$base" in
                  *-full-*) ;;
                  icode-*.tar.gz|icode-*.tgz)
                    echo "Warning: $base does not look like a -full- archive; slim tarballs may hit PyPI." >&2
                    ;;
                esac

                case "$src" in
                  *.tar.gz|*.tgz)
                    mkdir -p "$work/extract"
                    tar -xzf "$src" -C "$work/extract"
                    if [ -x "$work/extract/icode" ] || [ -f "$work/extract/icode" ]; then
                      icode="$work/extract/icode"
                    else
                      icode="$(find "$work/extract" -type f -name icode | head -n 1)"
                    fi
                    ;;
                  *)
                    icode="$src"
                    ;;
                esac
                run_icode
                ;;
            esac
          '''
{cred_close}        }}
      }}
    }}
    stage('Report') {{
      steps {{
        script {{
          currentBuild.description = "${{params.TASK}} | ${{params.EVAL_MODE}} | release=${{params.ICODE_RELEASE}} | git=${{params.ICODE_GIT_URL}}"
        }}
      }}
    }}
  }}

  post {{
    always {{
      archiveArtifacts artifacts: "**/reward.json", allowEmptyArchive: true
    }}
  }}
}}
"#,
        mode_first = mode_first,
        mode_second = mode_second,
        task = task,
        release = release,
        git_url = git_url,
        git_ref = git_ref,
        icode_args = icode_args,
        cred_open = cred_open,
        cred_close = cred_close,
    )
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

pub const ICODE_EVAL: &str = "icode_eval";

/// Ensure Pipeline job `icode_eval` (Pier + DeepSWE + iCode vs DeepSeek baseline).
pub fn ensure_icode_eval(
    jenkins_url: &str,
    api_user: &str,
    api_token_or_password: &str,
    credential_ids: &[String],
) -> Result<()> {
    let base = jenkins_url.trim_end_matches('/');
    let auth = format!("{api_user}:{api_token_or_password}");

    wait_for_jenkins(base, &auth, Duration::from_secs(90))?;

    let cookie_file = tempfile_path("mac-k3d-icode-eval-cookies")?;
    let crumb = fetch_crumb(base, &auth, &cookie_file);

    let exists = curl_status(
        base,
        &auth,
        &format!("/job/{ICODE_EVAL}/api/json"),
        &crumb,
        &cookie_file,
    )
    .map(|c| c == 200)
    .unwrap_or(false);

    let xml = icode_eval_job_xml(credential_ids);

    if exists {
        println!("Updating Jenkins job '{ICODE_EVAL}'…");
        let post_url = format!("{base}/job/{ICODE_EVAL}/config.xml");
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
            cmd: "curl config.xml icode_eval".into(),
            source: e.into(),
        })?;
        let _ = std::fs::remove_file(&cookie_file);
        let raw = String::from_utf8_lossy(&output.stdout);
        let code = raw.lines().last().unwrap_or("").trim().to_string();
        if code != "200" && code != "201" {
            println!("Warning: failed to update '{ICODE_EVAL}' (HTTP {code}).");
        } else {
            println!("Updated job '{ICODE_EVAL}'.");
        }
        return Ok(());
    }

    let create_url = format!(
        "{base}/createItem?name={}",
        urlencoding_simple(ICODE_EVAL)
    );
    println!("Creating Jenkins job '{ICODE_EVAL}' on {base} …");
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
        cmd: "curl createItem icode_eval".into(),
        source: e.into(),
    })?;
    let _ = std::fs::remove_file(&cookie_file);
    let raw = String::from_utf8_lossy(&output.stdout);
    let code = raw.lines().last().unwrap_or("").trim().to_string();
    if code != "200" && code != "201" && code != "302" && code != "303" {
        println!("Warning: failed to create '{ICODE_EVAL}' (HTTP {code}).");
        return Ok(());
    }
    println!(
        "Created job '{ICODE_EVAL}'.\n\
         Trigger: {base}/job/{ICODE_EVAL}/buildWithParameters  (or: mac-k3d eval)"
    );
    Ok(())
}

pub async fn ensure_icode_eval_from_cluster(
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
            println!("Skipping '{ICODE_EVAL}' create — could not read Jenkins admin password yet.");
            return Ok(());
        }
    };
    ensure_icode_eval(&url, "admin", &password, &credential_ids)
}

fn icode_eval_jenkinsfile(credential_ids: &[String]) -> String {
    let (cred_open, cred_close) = with_credentials_block(credential_ids);
    format!(
        r#"pipeline {{
  agent {{ label params.AGENT_LABEL }}

  options {{
    timeout(time: 12, unit: 'HOURS')
  }}

  parameters {{
    choice(name: 'HARNESS', choices: ['icode'], description: 'v1: icode only')
    choice(name: 'LLM', choices: ['deepseek'], description: 'v1: deepseek only')
    choice(name: 'BENCHMARK', choices: ['deepswe'], description: 'v1: deepswe only')
    string(name: 'N_TASKS', defaultValue: '1', description: 'Number of DeepSWE questions')
    choice(name: 'ICODE_MODE', choices: ['binary', 'source'], description: 'iCode delivery (users: binary drop)')
    string(name: 'ICODE_RELEASE', defaultValue: '', description: 'binary: empty = ~/.local/share/mac-k3d/icode or /opt/mac-k3d/icode')
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
            export ICODE_MODE="${{ICODE_MODE:-binary}}"
            export ICODE_RELEASE="${{ICODE_RELEASE:-}}"
            export ICODE_SOURCE="${{ICODE_SOURCE:-}}"
            export HARNESS=icode LLM=deepseek BENCHMARK=deepswe
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
"#
    )
}

fn icode_eval_job_xml(credential_ids: &[String]) -> String {
    let script = icode_eval_jenkinsfile(credential_ids);
    format!(
        r#"<?xml version='1.1' encoding='UTF-8'?>
<flow-definition plugin="workflow-job">
  <description>iCode is an agent harness. This job measures whether the harness helps an LLM on DeepSWE (via Pier): Arm A = iCode + LLM (DeepSeek); Arm B = the same LLM without iCode (baseline). Compare pass@1 / resolved / tokens / time in the archived JSON. See docs/binary-initializer/user-guide.md.</description>
  <keepDependencies>false</keepDependencies>
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
    fn jenkinsfile_dual_mode_binary_and_source() {
        let jf = jenkinsfile(&sample_opts());
        assert!(jf.contains("EVAL_MODE"));
        assert!(jf.contains("choice(name: 'EVAL_MODE'"));
        assert!(jf.contains("ICODE_RELEASE"));
        assert!(jf.contains("ICODE_GIT_URL"));
        assert!(jf.contains("ICODE_GIT_REF"));
        assert!(jf.contains("icode-linux-x86_64-full-v0.1.41.tar.gz"));
        assert!(jf.contains("tar -xzf"));
        assert!(jf.contains("ICODE_RELEASE is required"));
        assert!(jf.contains("PRIVATE-TOKEN"));
        assert!(jf.contains("GITCODE_TOKEN"));
        assert!(jf.contains("ICODE_ARGS"));
        assert!(jf.contains("ICODE_TASK"));
        assert!(jf.contains("lock(label: 'CPU_CORES'"));
        assert!(jf.contains("withCredentials"));
        assert!(jf.contains("deepseek-api-key"));
        assert!(jf.contains("source)"));
        assert!(jf.contains("( cd \"$srcdir\" && uv sync )"));
        assert!(jf.contains("git clone"));
        assert!(jf.contains("ICODE_GIT_URL is required when EVAL_MODE=source"));
        assert!(!jf.contains("honeyc"));
        assert!(!jf.contains("BINARY_TARGET"));
        assert!(!jf.contains("PYTHONPATH=."));
        assert!(!jf.contains("harbor run"));
        assert!(!jf.contains("GitSCM"));
        assert!(!jf.contains("eval --task"));
        let case_at = jf.find("case \"$mode\" in").expect("EVAL_MODE case");
        let uv_at = jf[case_at..].find("uv sync").expect("uv sync in Evaluate");
        let binary_req = jf[case_at..]
            .find("ICODE_RELEASE is required")
            .expect("binary require");
        assert!(
            uv_at < binary_req,
            "uv sync should appear in the source arm, before the binary require"
        );
    }

    #[test]
    fn jenkinsfile_choice_order_follows_default_eval_mode() {
        let mut opts = sample_opts();
        opts.default_eval_mode = "source".into();
        let jf = jenkinsfile(&opts);
        assert!(jf.contains("choices: ['source', 'binary']"));
        opts.default_eval_mode = "binary".into();
        let jf = jenkinsfile(&opts);
        assert!(jf.contains("choices: ['binary', 'source']"));
    }

    #[test]
    fn jenkinsfile_downloads_http_and_warns_slim() {
        let jf = jenkinsfile(&sample_opts());
        assert!(jf.contains("http://*|https://*"));
        assert!(jf.contains("icode-in"));
        assert!(jf.contains("-full-"));
        assert!(jf.contains("slim tarballs"));
    }

    #[test]
    fn job_xml_wraps_pipeline_in_cdata() {
        let xml = job_config_xml(&sample_opts());
        assert!(xml.contains("<![CDATA["));
        assert!(xml.contains("flow-definition"));
        assert!(xml.contains("<name>EVAL_MODE</name>"));
        assert!(xml.contains("<name>ICODE_RELEASE</name>"));
        assert!(xml.contains("<name>ICODE_GIT_URL</name>"));
        assert!(xml.contains("<name>ICODE_GIT_REF</name>"));
        assert!(xml.contains("<name>ICODE_ARGS</name>"));
        assert!(xml.contains("<string>binary</string>"));
        assert!(!xml.contains("<name>HONEYC_BIN</name>"));
        assert!(!xml.contains("<name>BINARY_TARGET</name>"));
        assert!(!xml.contains("<name>LOLBENCH_PATH</name>"));
        assert!(xml.contains("pipeline {"));
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
    fn icode_eval_xml_mentions_progress_and_scripts() {
        let xml = icode_eval_job_xml(&["deepseek-api-key".into()]);
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
        assert!(xml.contains("mac-k3d config -c worker.yaml"));
        assert!(xml.contains("eval --stage p0"));
        assert!(xml.contains("agent harness"));
        assert!(xml.contains("without"));
        assert!(xml.contains("user-guide"));
        assert!(
            !xml.contains("/home/Toby/Documents/Toby/iCode-main"),
            "job must not hardcode a lab iCode path"
        );
    }

    #[test]
    fn icode_eval_xml_omits_bind_when_credential_ids_empty() {
        let xml = icode_eval_job_xml(&[]);
        assert!(
            !xml.contains("withCredentials"),
            "empty IDs must omit the bind"
        );
    }

    #[test]
    fn skip_secrets_must_pass_listed_ids_to_keep_bind() {
        // --skip-secrets / start list existing IDs then call this helper; [] would strip the bind.
        let xml = icode_eval_job_xml(&["deepseek-api-key".into()]);
        assert!(xml.contains("withCredentials"));
        assert!(xml.contains("deepseek-api-key"));
        assert!(xml.contains("DEEPSEEK_API_KEY"));
    }
}
