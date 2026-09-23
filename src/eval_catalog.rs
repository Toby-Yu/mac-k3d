//! Allow-lists for eval YAML / Jenkins parameters.
//!
//! v1 runners are `icode` + `deepseek` on `deepswe` | `lolbench` | `swebenchpro`. Chat Completions
//! ids live in `MODELS` (not `LLMS`). Append here when a new harness, LLM, model,
//! or benchmark is implemented.

use crate::error::{Error, Result};

pub const HARNESSES: &[&str] = &["icode"];
pub const LLMS: &[&str] = &["deepseek"];
pub const BENCHMARKS: &[&str] = &["deepswe", "lolbench", "swebenchpro"];
/// DeepSeek Chat Completions ids (`DEEPSEEK_MODEL` / `--model`). First is the default.
/// Append only `data[].id` values from `GET https://api.deepseek.com/models`.
/// Product name "V4.1 Flash" is API id `deepseek-flash` (not `deepseek-v4.1-flash`).
pub const MODELS: &[&str] = &["deepseek-v4-pro", "deepseek-flash"];
/// Jenkins / CLI iCode inputs (`ICODE_MODE`). `binary` is a pipeline alias of `release`.
pub const ICODE_CI_MODES: &[&str] = &["release", "git"];
/// https hosts allowed for `ICODE_GIT_URL` / `get_bin_icode`.
pub const ICODE_GIT_HOSTS: &[&str] = &["github.com", "www.github.com", "gitcode.com"];
/// How `ICODE_GIT_REF` is checked out. Jenkins/CLI pick branch, tag, or commit (`auto` is leftover).
pub const ICODE_GIT_REF_KINDS: &[&str] = &["branch", "tag", "commit"];

pub fn join_allowed(items: &[&str]) -> String {
    items.join(", ")
}

pub fn parse_task_list(raw: &str) -> Vec<String> {
    raw.split([',', '\n'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

pub fn require_harness(value: &str) -> Result<String> {
    require_in(value, HARNESSES, "harness")
}

pub fn require_llm(value: &str) -> Result<String> {
    require_in(value, LLMS, "llm")
}

pub fn require_benchmark(value: &str) -> Result<String> {
    require_in(value, BENCHMARKS, "benchmark")
}

pub fn require_model(value: &str) -> Result<String> {
    require_in(value, MODELS, "model")
}

pub fn normalize_icode_mode(value: &str) -> Result<String> {
    let v = value.trim().to_ascii_lowercase();
    match v.as_str() {
        "binary" | "release" => Ok("release".into()),
        "git" => Ok("git".into()),
        "source" => Err(Error::Config(
            "icode-mode source is not supported. Push to GitHub/GitCode and use --icode-mode git, or download a *-full-* binary and use --icode-mode release".into(),
        )),
        _ => Err(Error::Config(format!(
            "unknown icode-mode '{value}' (allowed: release, git; binary is an alias of release)"
        ))),
    }
}

pub fn require_icode_git_url(value: &str) -> Result<String> {
    let u = value.trim();
    if u.is_empty() {
        return Err(Error::Config("ICODE_GIT_URL is empty".into()));
    }
    let rest = u
        .strip_prefix("https://")
        .ok_or_else(|| Error::Config("ICODE_GIT_URL must be https://".into()))?;
    if rest.contains('@') {
        return Err(Error::Config(
            "ICODE_GIT_URL must not contain userinfo (no tokens in the URL)".into(),
        ));
    }
    let host = rest
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    if !ICODE_GIT_HOSTS.iter().any(|h| *h == host) {
        return Err(Error::Config(format!(
            "ICODE_GIT_URL host '{host}' is not allow-listed ({})",
            join_allowed(ICODE_GIT_HOSTS)
        )));
    }
    Ok(u.to_string())
}

pub fn require_icode_git_ref(value: &str) -> Result<String> {
    let r = value.trim();
    if r.is_empty() {
        return Err(Error::Config("ICODE_GIT_REF is empty".into()));
    }
    if r.starts_with('-') || r.contains("..") {
        return Err(Error::Config(
            "ICODE_GIT_REF must not start with - or contain ..".into(),
        ));
    }
    if !r
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '+' | '-'))
    {
        return Err(Error::Config(
            "ICODE_GIT_REF is not a tag, commit, or branch name".into(),
        ));
    }
    Ok(r.to_string())
}

pub fn looks_like_git_commit(value: &str) -> bool {
    let r = value.trim();
    (7..=40).contains(&r.len()) && r.chars().all(|c| c.is_ascii_hexdigit())
}

/// Normalize `ICODE_GIT_REF_KIND`. `auto` infers commit vs branch from the ref.
pub fn normalize_icode_git_ref_kind(kind: &str, git_ref: &str) -> Result<String> {
    let k = kind.trim().to_ascii_lowercase();
    match k.as_str() {
        "branch" | "tag" | "commit" => Ok(k),
        "auto" | "" => {
            if looks_like_git_commit(git_ref) {
                Ok("commit".into())
            } else {
                Ok("branch".into())
            }
        }
        _ => Err(Error::Config(format!(
            "unknown icode-git-ref-kind '{kind}' (allowed: {}; leftover auto still maps from the ref)",
            join_allowed(ICODE_GIT_REF_KINDS)
        ))),
    }
}

pub fn require_icode_git_ref_for_kind(kind: &str, git_ref: &str) -> Result<(String, String)> {
    let git_ref = require_icode_git_ref(git_ref)?;
    let kind = normalize_icode_git_ref_kind(kind, &git_ref)?;
    if kind == "commit" && !looks_like_git_commit(&git_ref) {
        return Err(Error::Config(format!(
            "ICODE_GIT_REF_KIND=commit requires a git SHA (7–40 hex chars), not '{git_ref}'. Use --icode-git-ref-kind branch or tag."
        )));
    }
    Ok((kind, git_ref))
}

/// Env / Jenkins binding for a private git clone. Never put the PAT in the URL.
pub struct GitPatSpec {
    pub env_var: &'static str,
    pub env_alias: &'static str,
    pub jenkins_id: &'static str,
    pub prompt: &'static str,
}

pub fn icode_git_host(url: &str) -> Result<String> {
    let u = require_icode_git_url(url)?;
    let host = u
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    Ok(host.to_string())
}

pub fn git_pat_spec(url: &str) -> Result<GitPatSpec> {
    match icode_git_host(url)?.as_str() {
        "gitcode.com" => Ok(GitPatSpec {
            env_var: "GITCODE_TOKEN",
            env_alias: "MAC_K3D_GITCODE_PAT",
            jenkins_id: "gitcode-pat",
            prompt: "GitCode PAT (empty if the repo is public; saved to .env, never printed)",
        }),
        "github.com" | "www.github.com" => Ok(GitPatSpec {
            env_var: "GITHUB_TOKEN",
            env_alias: "MAC_K3D_GITHUB_PAT",
            jenkins_id: "github-pat",
            prompt: "GitHub PAT (empty if the repo is public; saved to .env, never printed)",
        }),
        other => Err(Error::Config(format!(
            "ICODE_GIT_URL host '{other}' is not allow-listed ({})",
            join_allowed(ICODE_GIT_HOSTS)
        ))),
    }
}

pub fn default_model() -> &'static str {
    MODELS[0]
}

fn require_in(value: &str, allowed: &[&str], kind: &str) -> Result<String> {
    let v = value.trim().to_ascii_lowercase();
    if allowed.iter().any(|a| *a == v) {
        return Ok(v);
    }
    Err(Error::Config(format!(
        "unknown {kind} '{value}' (allowed: {})",
        join_allowed(allowed)
    )))
}

/// Put `preferred` first when it is in `allowed` (Jenkins choice default).
pub fn choices_preferred_first<'a>(allowed: &'a [&'a str], preferred: &str) -> Vec<&'a str> {
    let pref = preferred.trim();
    let mut out = Vec::new();
    if let Some(hit) = allowed.iter().copied().find(|a| *a == pref) {
        out.push(hit);
    }
    for a in allowed {
        if !out.contains(a) {
            out.push(*a);
        }
    }
    out
}

pub fn print_catalog() {
    println!("harness:    {}", join_allowed(HARNESSES));
    println!("llm:        {}", join_allowed(LLMS));
    println!("model:      {}  (default first)", join_allowed(MODELS));
    println!("benchmark:  {}", join_allowed(BENCHMARKS));
    println!("questions:  --task ID  |  --n-tasks N  |  --tasks id,id,…  (mutually exclusive)");
    println!("icode:      release | git  (Jenkins + CLI; persist binary path with mac-k3d set --icode-release)");
    println!("git hosts:  {}", join_allowed(ICODE_GIT_HOSTS));
    println!(
        "git ref:    {}  (legacy auto: 7-40 hex -> commit, else branch)",
        join_allowed(ICODE_GIT_REF_KINDS)
    );
    println!(
        "provider:   GET {{ICODE_API_BASE or https://api.deepseek.com}}/models  (set --check-models)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_unknown_harness() {
        let err = require_harness("oracle").unwrap_err().to_string();
        assert!(err.contains("allowed: icode"), "{err}");
    }

    #[test]
    fn reject_unknown_model() {
        let err = require_model("gpt-4").unwrap_err().to_string();
        assert!(
            err.contains("allowed: deepseek-v4-pro, deepseek-flash"),
            "{err}"
        );
    }

    #[test]
    fn require_model_accepts_flash() {
        assert_eq!(require_model("deepseek-flash").unwrap(), "deepseek-flash");
        assert_eq!(default_model(), "deepseek-v4-pro");
    }

    #[test]
    fn parse_comma_and_newline_list() {
        assert_eq!(
            parse_task_list(" abs-stepped-slices , abs-module-cache-flags\n"),
            vec!["abs-stepped-slices", "abs-module-cache-flags"]
        );
    }

    #[test]
    fn preferred_first_orders_choices() {
        assert_eq!(
            choices_preferred_first(BENCHMARKS, "lolbench"),
            vec!["lolbench", "deepswe", "swebenchpro"]
        );
        assert_eq!(
            choices_preferred_first(ICODE_CI_MODES, "git"),
            vec!["git", "release"]
        );
    }

    #[test]
    fn require_icode_git_url_rejects_file_and_userinfo() {
        assert!(require_icode_git_url("file:///tmp/icode").is_err());
        assert!(require_icode_git_url("https://user:token@github.com/org/icode.git").is_err());
        assert!(require_icode_git_url("https://github.com.evil.com/org/icode.git").is_err());
        assert!(require_icode_git_url("https://github.com/org/icode.git").is_ok());
    }

    #[test]
    fn require_icode_git_ref_rejects_flags() {
        assert!(require_icode_git_ref("--upload-pack").is_err());
        assert!(require_icode_git_ref("foo;rm").is_err());
        assert_eq!(require_icode_git_ref("v0.1.41").unwrap(), "v0.1.41");
        assert_eq!(normalize_icode_mode("binary").unwrap(), "release");
        assert_eq!(normalize_icode_mode("git").unwrap(), "git");
        let err = normalize_icode_mode("source").unwrap_err().to_string();
        assert!(err.contains("not supported"), "{err}");
    }

    #[test]
    fn git_ref_kind_auto_and_commit_sha() {
        assert_eq!(
            normalize_icode_git_ref_kind("auto", "main").unwrap(),
            "branch"
        );
        assert_eq!(
            normalize_icode_git_ref_kind("auto", "0123456789abcdef0123456789abcdef01234567")
                .unwrap(),
            "commit"
        );
        assert_eq!(
            normalize_icode_git_ref_kind("tag", "v0.1.41").unwrap(),
            "tag"
        );
        assert!(require_icode_git_ref_for_kind("commit", "main").is_err());
        let (k, r) = require_icode_git_ref_for_kind("commit", "deadbee").unwrap();
        assert_eq!(k, "commit");
        assert_eq!(r, "deadbee");
        assert!(normalize_icode_git_ref_kind("sha", "main").is_err());
        assert!(
            !ICODE_GIT_REF_KINDS.contains(&"auto"),
            "Jenkins/CLI catalog is branch, tag, commit"
        );
    }

    #[test]
    fn git_pat_spec_maps_host_to_env_and_jenkins_id() {
        let gc = git_pat_spec("https://gitcode.com/org/icode.git").unwrap();
        assert_eq!(gc.env_var, "GITCODE_TOKEN");
        assert_eq!(gc.jenkins_id, "gitcode-pat");
        let gh = git_pat_spec("https://github.com/org/icode.git").unwrap();
        assert_eq!(gh.env_var, "GITHUB_TOKEN");
        assert_eq!(gh.jenkins_id, "github-pat");
        assert!(git_pat_spec("https://user:token@gitcode.com/org/icode.git").is_err());
    }
}
