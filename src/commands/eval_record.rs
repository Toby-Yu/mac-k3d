//! `mac-k3d eval record`: what each question did in one run, written to
//! docs/testing/question-log.md by pipeline/lib/question_log.py.
//!
//! A Jenkins build is read through the controller's REST API (build info,
//! consoleText, the archived artifact.json). The API user and token go to curl
//! on stdin (`--config -`), never in its arguments, a URL, a file or a log.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use clap::Args;

use crate::config::MacK3dConfig;
use crate::error::{Error, Result};

const LOG_DOC: &str = "docs/testing/question-log.md";
const SCRIPT: &str = "pipeline/lib/question_log.py";
pub(super) const CONSOLE_LOG: &str = "last_console.log";

#[derive(Debug, Default, Clone, Args)]
pub struct RecordArgs {
    /// Jenkins job of the build to record, e.g. deepswe_one_task (needs --build)
    #[arg(long, requires = "build", conflicts_with = "run")]
    pub job: Option<String>,

    /// Build number of --job
    #[arg(long, requires = "job")]
    pub build: Option<u32>,

    /// A local run's output folder (holds artifact.json). Default: the last local run
    #[arg(long)]
    pub run: Option<PathBuf>,

    /// Eval workdir of the last local run (default: ./eval-runs)
    #[arg(long)]
    pub workdir: Option<PathBuf>,
}

/// How a local run that just finished ended.
pub(super) struct LocalOutcome {
    pub exit_code: Option<i32>,
    pub started_epoch: u64,
}

pub(super) fn run(args: RecordArgs, config: &MacK3dConfig) -> Result<()> {
    let repo = docs_repo(&super::eval::discover_repo_root()?)?;
    if let (Some(job), Some(build)) = (args.job.as_deref(), args.build) {
        return record_jenkins(&repo, config, job, build);
    }
    let workdir = args
        .workdir
        .or_else(|| std::env::var_os("MAC_K3D_EVAL_WORKDIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("eval-runs"));
    record_local(&repo, &resolve(&repo, &workdir), args.run.as_deref(), None)
}

/// The checkout whose docs/testing/ holds the log; the extracted share copy has none.
pub(super) fn docs_repo(root: &Path) -> Result<PathBuf> {
    if root.join("docs/testing").is_dir() && root.join(SCRIPT).is_file() {
        return Ok(root.to_path_buf());
    }
    Err(Error::Config(format!(
        "{} has no docs/testing/ or {SCRIPT}; run `mac-k3d eval record` from your mac-k3d checkout",
        root.display()
    )))
}

fn resolve(repo: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    }
}

/// The last local run (`last_output.txt`, `last_console.log`) or one output folder.
pub(super) fn record_local(
    repo: &Path,
    workdir: &Path,
    run_dir: Option<&Path>,
    outcome: Option<&LocalOutcome>,
) -> Result<()> {
    let mut args: Vec<String> = Vec::new();
    match run_dir {
        Some(dir) => {
            let artifact = resolve(repo, dir).join("artifact.json");
            if !artifact.is_file() {
                return Err(Error::Config(format!("no {}", artifact.display())));
            }
            args.extend(["--artifact".into(), artifact.display().to_string()]);
        }
        None => {
            if let Some(dir) = last_output_dir(repo, workdir) {
                let artifact = dir.join("artifact.json");
                if artifact.is_file() {
                    args.extend(["--artifact".into(), artifact.display().to_string()]);
                }
            }
            let console = workdir.join(CONSOLE_LOG);
            if console.is_file() {
                args.extend(["--console".into(), console.display().to_string()]);
            }
        }
    }
    if let Some(done) = outcome {
        let code = done
            .exit_code
            .map_or_else(|| "signal".to_string(), |c| c.to_string());
        args.extend(["--result".into(), format!("exit {code}")]);
        args.extend(["--date".into(), done.started_epoch.to_string()]);
    }
    run_question_log(repo, &args)
}

/// `last_output.txt` names the run folder as `<dir>/**` (report/render).
fn last_output_dir(repo: &Path, workdir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(workdir.join("last_output.txt")).ok()?;
    let rel = text.trim().trim_end_matches("/**").trim();
    (!rel.is_empty()).then(|| resolve(repo, Path::new(rel)))
}

fn run_question_log(repo: &Path, extra: &[String]) -> Result<()> {
    let script = repo.join(SCRIPT);
    let status = Command::new("python3")
        .arg(&script)
        .arg("record")
        .arg("--repo")
        .arg(repo)
        .args(extra)
        .status()
        .map_err(|e| Error::CommandFailed {
            cmd: format!("python3 {}", script.display()),
            source: e.into(),
        })?;
    if !status.success() {
        return Err(Error::CommandFailed {
            cmd: format!("python3 {} record", script.display()),
            source: anyhow::anyhow!("exit {:?}", status.code()),
        });
    }
    println!("Write what you changed in the `fix` column of {LOG_DOC}.");
    Ok(())
}

struct JenkinsAccess {
    base: String,
    user: String,
    token: String,
    source: String,
}

/// The loaded config's `jenkins_agent` API user/token, else worker.yaml's (a
/// PC that is both a standalone controller and a worker keeps them there).
fn jenkins_access(config: &MacK3dConfig) -> Result<JenkinsAccess> {
    let worker_path = MacK3dConfig::default_worker_path();
    let worker = MacK3dConfig::load_file(&worker_path).ok();
    let candidates = [
        Some((config, "the loaded config".to_string())),
        worker.as_ref().map(|w| (w, worker_path.display().to_string())),
    ];
    for (cfg, source) in candidates.into_iter().flatten() {
        let Some((user, token)) = cfg.jenkins_agent.api_credentials() else {
            continue;
        };
        let base = cfg
            .jenkins_agent
            .controller_url
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("http://localhost:{}", cfg.jenkins.host_port));
        return Ok(JenkinsAccess {
            base: base.trim_end_matches('/').to_string(),
            user: user.to_string(),
            token: token.to_string(),
            source,
        });
    }
    Err(Error::Config(format!(
        "no Jenkins API user/token: set jenkins_agent.api_user and api_token in the config \
         or in {}",
        worker_path.display()
    )))
}

/// curl's config text for `--config -`: the only place the credentials go.
fn curl_auth_config(user: &str, token: &str) -> String {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("user = \"{}:{}\"\n", esc(user), esc(token))
}

/// curl arguments for one GET; credentials come from stdin, never from here.
fn curl_get_args(url: &str, out: &Path) -> Vec<String> {
    vec![
        "-fsS".into(),
        "-g".into(),
        "--max-time".into(),
        "300".into(),
        "--config".into(),
        "-".into(),
        "-o".into(),
        out.display().to_string(),
        url.into(),
    ]
}

fn curl_get(access: &JenkinsAccess, url: &str, out: &Path) -> Result<()> {
    let mut child = Command::new("curl")
        .args(curl_get_args(url, out))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::CommandFailed {
            cmd: "curl".into(),
            source: e.into(),
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(curl_auth_config(&access.user, &access.token).as_bytes())
            .map_err(|e| Error::Config(format!("curl stdin: {e}")))?;
    }
    let done = child.wait_with_output().map_err(|e| Error::CommandFailed {
        cmd: "curl".into(),
        source: e.into(),
    })?;
    if !done.status.success() {
        let err = String::from_utf8_lossy(&done.stderr);
        let first = err.lines().next().unwrap_or("curl failed").trim();
        let hint = if first.contains("error: 404") {
            " (no such job or build on this controller)"
        } else if first.contains("error: 401") || first.contains("error: 403") {
            " (Jenkins rejected the API user/token)"
        } else {
            ""
        };
        return Err(Error::Config(format!("GET {url} failed: {first}{hint}")));
    }
    Ok(())
}

fn url_path(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn build_url(base: &str, job: &str, build: u32, rest: &str) -> String {
    format!("{base}/job/{}/{build}/{rest}", url_path(job))
}

/// The run's artifact.json among the build's archived files; a dispatcher's
/// aggregate wins over its shards'.
fn pick_artifact(paths: &[String]) -> Option<&str> {
    let all: Vec<&str> = paths
        .iter()
        .map(String::as_str)
        .filter(|p| *p == "artifact.json" || p.ends_with("/artifact.json"))
        .collect();
    all.iter()
        .find(|p| p.starts_with("aggregate/") || p.contains("/aggregate/"))
        .or_else(|| all.first())
        .copied()
}

fn job_benchmark(job: &str) -> &str {
    let head = job.split('_').next().unwrap_or("");
    if crate::eval_catalog::BENCHMARKS.contains(&head) {
        head
    } else {
        ""
    }
}

/// Removes the download folder however the recording ends.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn record_jenkins(repo: &Path, config: &MacK3dConfig, job: &str, build: u32) -> Result<()> {
    let access = jenkins_access(config)?;
    println!(
        "Reading {job} #{build} from {} (API user from {})",
        access.base, access.source
    );
    let tmp = TempDir(std::env::temp_dir().join(format!(
        "mac-k3d-record-{}-{job}-{build}",
        std::process::id()
    )));
    std::fs::create_dir_all(&tmp.0)
        .map_err(|e| Error::Config(format!("mkdir {}: {e}", tmp.0.display())))?;

    let info_path = tmp.0.join("build.json");
    curl_get(
        &access,
        &build_url(
            &access.base,
            job,
            build,
            "api/json?tree=number,result,building,timestamp,builtOn,artifacts[relativePath]",
        ),
        &info_path,
    )?;
    let info: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&info_path)
            .map_err(|e| Error::Config(format!("read {}: {e}", info_path.display())))?,
    )
    .map_err(|e| Error::Config(format!("{job} #{build}: build info is not JSON ({e})")))?;
    if info["building"].as_bool() == Some(true) {
        return Err(Error::Config(format!(
            "{job} #{build} is still running; record it after it finishes"
        )));
    }

    let console = tmp.0.join("console.txt");
    curl_get(
        &access,
        &build_url(&access.base, job, build, "consoleText"),
        &console,
    )?;

    let paths: Vec<String> = info["artifacts"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x["relativePath"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut args = vec![
        "--run".to_string(),
        format!("{job} #{build}"),
        "--build".into(),
        build.to_string(),
        "--console".into(),
        console.display().to_string(),
        "--result".into(),
        info["result"].as_str().unwrap_or("").to_string(),
        "--worker".into(),
        info["builtOn"].as_str().unwrap_or("").to_string(),
        "--benchmark".into(),
        job_benchmark(job).to_string(),
    ];
    if let Some(ms) = info["timestamp"].as_u64() {
        args.extend(["--date".into(), ms.to_string()]);
    }
    if let Some(rel) = pick_artifact(&paths) {
        let artifact = tmp.0.join("artifact.json");
        curl_get(
            &access,
            &build_url(&access.base, job, build, &format!("artifact/{}", url_path(rel))),
            &artifact,
        )?;
        args.extend(["--artifact".into(), artifact.display().to_string()]);
    }
    run_question_log(repo, &args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_go_to_the_curl_config_not_the_arguments() {
        let token = "11abcdef0123456789abcdef0123456789";
        let args = curl_get_args(
            "http://ctl:17070/job/deepswe_one_task/54/consoleText",
            Path::new("/tmp/x"),
        );
        assert!(args.iter().all(|a| !a.contains(token) && !a.contains("admin")));
        assert!(args.windows(2).any(|w| w[0] == "--config" && w[1] == "-"));
        assert!(args.contains(&"-g".to_string()), "brackets in ?tree= must not glob");
        assert_eq!(
            curl_auth_config("admin", token),
            format!("user = \"admin:{token}\"\n")
        );
        assert_eq!(
            curl_auth_config("a\"b", "c\\d"),
            "user = \"a\\\"b:c\\\\d\"\n"
        );
    }

    #[test]
    fn build_urls_carry_no_credentials() {
        let url = build_url("http://ctl:17070", "deepswe_one_task", 54, "consoleText");
        assert_eq!(url, "http://ctl:17070/job/deepswe_one_task/54/consoleText");
        let art = build_url(
            "http://ctl:17070",
            "deepswe_one_task",
            54,
            &format!("artifact/{}", url_path("eval-runs/output/deepswe/54 x/artifact.json")),
        );
        assert_eq!(
            art,
            "http://ctl:17070/job/deepswe_one_task/54/artifact/eval-runs/output/deepswe/54%20x/artifact.json"
        );
        assert!(!url.contains('@'));
    }

    #[test]
    fn pick_artifact_prefers_the_aggregate() {
        let shard = "eval-runs/output/deepswe/run-1/artifact.json".to_string();
        let agg = "aggregate/artifact.json".to_string();
        let other = "eval-runs/eval_protocol_inputs.json".to_string();
        assert_eq!(pick_artifact(std::slice::from_ref(&other)), None);
        assert_eq!(
            pick_artifact(&[other.clone(), shard.clone()]),
            Some(shard.as_str())
        );
        assert_eq!(pick_artifact(&[shard, agg.clone()]), Some(agg.as_str()));
    }

    #[test]
    fn job_benchmark_reads_the_job_prefix() {
        assert_eq!(job_benchmark("deepswe_one_task"), "deepswe");
        assert_eq!(job_benchmark("lolbench_some_task"), "lolbench");
        assert_eq!(job_benchmark("swebenchpro_full_suite_task"), "swebenchpro");
        assert_eq!(job_benchmark("other"), "");
    }

    #[test]
    fn last_output_dir_strips_the_archive_glob() {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("eval-runs");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(
            work.join("last_output.txt"),
            "eval-runs/output/deepswe/run-1/**\n",
        )
        .unwrap();
        assert_eq!(
            last_output_dir(dir.path(), &work),
            Some(dir.path().join("eval-runs/output/deepswe/run-1"))
        );
        std::fs::write(work.join("last_output.txt"), "/abs/run-2/**\n").unwrap();
        assert_eq!(
            last_output_dir(dir.path(), &work),
            Some(PathBuf::from("/abs/run-2"))
        );
    }

    #[test]
    fn docs_repo_needs_the_docs_and_the_script() {
        let dir = tempfile::tempdir().unwrap();
        assert!(docs_repo(dir.path()).is_err());
        std::fs::create_dir_all(dir.path().join("docs/testing")).unwrap();
        std::fs::create_dir_all(dir.path().join("pipeline/lib")).unwrap();
        std::fs::write(dir.path().join(SCRIPT), "").unwrap();
        assert_eq!(docs_repo(dir.path()).unwrap(), dir.path());
    }
}
