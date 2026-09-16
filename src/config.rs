use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cli::JenkinsMode;
use crate::error::{Error, Result};

/// User-facing configuration loaded from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MacK3dConfig {
    /// Machine role from prepare wizard.
    pub role: NodeRole,
    /// Host OS recorded by prepare (`macos` | `linux`); informational.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    pub cluster: ClusterConfig,
    pub jenkins: JenkinsConfig,
    pub docker: DockerConfig,
    pub storage: StorageConfig,
    pub dependencies: DependenciesConfig,
    pub lolbench: LolbenchConfig,
    pub jenkins_agent: JenkinsAgentConfig,
    pub jenkins_job: JenkinsJobConfig,
    pub resources: ResourcesConfig,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    #[default]
    Standalone,
    Controller,
    Worker,
}

/// Where large artifacts and caches are stored (set by `prepare` wizard).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub k3d: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jenkins: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<PathBuf>,
}

/// How each external tool is resolved (discovered, installed, or skipped).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DependenciesConfig {
    pub docker: DependencyEntry,
    pub k3d: DependencyEntry,
    pub kubectl: DependencyEntry,
    pub helm: DependencyEntry,
    pub harbor: DependencyEntry,
    pub java: DependencyEntry,
}

impl Default for DependenciesConfig {
    fn default() -> Self {
        Self {
            docker: DependencyEntry::default(),
            k3d: DependencyEntry::default(),
            kubectl: DependencyEntry::default(),
            helm: DependencyEntry {
                source: DependencySource::Skip,
                ..DependencyEntry::default()
            },
            harbor: DependencyEntry {
                source: DependencySource::Skip,
                ..DependencyEntry::default()
            },
            java: DependencyEntry {
                source: DependencySource::Skip,
                ..DependencyEntry::default()
            },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DependencyEntry {
    pub source: DependencySource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DependencySource {
    Existing,
    #[default]
    Install,
    Skip,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClusterConfig {
    pub name: String,
    pub agents: u8,
    pub ports: Vec<PortMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host: u16,
    pub container: u16,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct JenkinsConfig {
    pub enabled: bool,
    pub namespace: String,
    pub release_name: String,
    pub host_port: u16,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerConfig {
    pub startup_timeout_secs: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LolbenchConfig {
    /// Path to LoLBench-Preview checkout (optional on standalone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub source: LolbenchSource,
    /// Git remote used when cloning (printed/used by prepare).
    pub git_url: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LolbenchSource {
    #[default]
    Skip,
    Existing,
    Clone,
    Release,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct JenkinsAgentConfig {
    /// Worker only: Jenkins controller base URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_fs: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_jar: Option<PathBuf>,
    /// Logical CPU cores recorded at prepare time.
    pub cpu_cores: u32,
    /// Jenkins user for REST API (plaintext for now; encrypt later).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_user: Option<String>,
    /// Jenkins API token (plaintext for now; encrypt later).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
}

/// Non-secret defaults for the `lolbench_one_task` Pipeline job (controller).
///
/// Secrets are never stored here — see `docs/secrets.md` and pending credentials file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct JenkinsJobConfig {
    pub default_task: String,
    /// `binary` (GitCode `-full-` tarball) or `source` (git clone + uv sync in the job).
    pub default_eval_mode: String,
    /// GitCode `icode-<os>-<arch>-full-vX.Y.Z.tar.gz` URL or path (or a stub `icode` binary).
    pub default_icode_release: String,
    /// iCode git URL for `EVAL_MODE=source` (private GitCode clone uses `gitcode-pat`).
    pub default_icode_git_url: String,
    /// Git ref for source mode (branch or tag). Empty becomes `main`.
    pub default_icode_git_ref: String,
    pub default_icode_args: String,
    /// Deprecated; ignored if `default_icode_release` is set.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_binary_target: String,
    /// Deprecated honeyc-era field; ignored by the job generator.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_honeyc_bin: String,
    /// Deprecated honeyc-era field; ignored by the job generator.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_honeyc_args: String,
    /// Eval harness id (`mac-k3d set --harness`). Catalog: `icode`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_harness: String,
    /// Deprecated alias; `default_llm` is the live field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_model: String,
    /// Eval LLM family (`mac-k3d set --llm`). Catalog: `deepseek`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_llm: String,
    /// Which Jenkins job receives TASK / N_TASKS / TASKS defaults (`deepswe` | `lolbench`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_benchmark: String,
    /// First N sorted questions when `default_task` and `default_tasks` are empty.
    #[serde(default = "default_n_tasks_one")]
    pub default_n_tasks: u32,
    /// Explicit question ids (`mac-k3d set --tasks a,b`). Overrides N and single TASK.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_tasks: Vec<String>,
}

fn default_n_tasks_one() -> u32 {
    1
}

impl Default for JenkinsJobConfig {
    fn default() -> Self {
        Self {
            default_task: "ruff_1".into(),
            default_eval_mode: "binary".into(),
            default_icode_release: String::new(),
            default_icode_git_url: String::new(),
            default_icode_git_ref: "main".into(),
            default_icode_args: String::new(),
            default_binary_target: String::new(),
            default_honeyc_bin: String::new(),
            default_honeyc_args: String::new(),
            default_harness: String::new(),
            default_model: String::new(),
            default_llm: String::new(),
            default_benchmark: String::new(),
            default_n_tasks: 1,
            default_tasks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ResourcesConfig {
    /// Jenkins Lockable Resources label for CPU capacity.
    pub cpu_cores_label: String,
    /// Minimum free disk (GB) required by prepare for this role (0 = use role default).
    pub disk_min_gb: u64,
}

impl Default for ResourcesConfig {
    fn default() -> Self {
        Self {
            cpu_cores_label: "CPU_CORES".into(),
            disk_min_gb: 0,
        }
    }
}

impl Default for MacK3dConfig {
    fn default() -> Self {
        Self {
            role: NodeRole::Standalone,
            platform: None,
            cluster: ClusterConfig {
                name: "mac-k3d".into(),
                agents: 0,
                ports: default_ports(),
            },
            jenkins: JenkinsConfig {
                enabled: false,
                namespace: "jenkins".into(),
                release_name: "jenkins".into(),
                host_port: 17070,
            },
            docker: DockerConfig {
                startup_timeout_secs: 120,
            },
            storage: StorageConfig::default(),
            dependencies: DependenciesConfig::default(),
            lolbench: LolbenchConfig {
                path: None,
                source: LolbenchSource::Skip,
                git_url: "https://github.com/MichaelLing83/LoLBench-Preview.git".into(),
            },
            jenkins_agent: JenkinsAgentConfig {
                controller_url: None,
                name: None,
                labels: crate::platform::default_agent_labels(),
                remote_fs: None,
                agent_jar: None,
                cpu_cores: 0,
                api_user: None,
                api_token: None,
            },
            jenkins_job: JenkinsJobConfig::default(),
            resources: ResourcesConfig::default(),
        }
    }
}

impl StorageConfig {
    pub fn resolve(&self, sub: &str, explicit: &Option<PathBuf>) -> Option<PathBuf> {
        explicit
            .clone()
            .or_else(|| self.base_dir.as_ref().map(|b| b.join(sub)))
    }

    pub fn docker_dir(&self) -> Option<PathBuf> {
        self.resolve("docker", &self.docker)
    }

    pub fn k3d_dir(&self) -> Option<PathBuf> {
        self.resolve("k3d", &self.k3d)
    }

    pub fn jenkins_dir(&self) -> Option<PathBuf> {
        self.resolve("jenkins", &self.jenkins)
    }

    pub fn downloads_dir(&self) -> Option<PathBuf> {
        self.resolve("downloads", &self.downloads)
    }
}

fn default_ports() -> Vec<PortMapping> {
    vec![
        PortMapping {
            host: 8080,
            container: 80,
        },
        PortMapping {
            host: 8443,
            container: 443,
        },
    ]
}

impl MacK3dConfig {
    pub fn config_dir() -> PathBuf {
        home_dir()
            .map(|h| h.join(".config").join("mac-k3d"))
            .unwrap_or_else(|| PathBuf::from(".mac-k3d"))
    }

    pub fn default_config_path() -> PathBuf {
        Self::config_dir().join("config.yaml")
    }

    pub fn default_worker_path() -> PathBuf {
        Self::config_dir().join("worker.yaml")
    }

    /// `-c` wins. Else `config.yaml` if it exists, else `worker.yaml` (clean worker-only machines).
    pub fn resolve_config_path(explicit: Option<&Path>) -> PathBuf {
        resolve_config_path_in(&Self::config_dir(), explicit)
    }

    /// Pending CI secrets from prepare (0600 YAML); consumed by `config` into Jenkins Credentials.
    pub fn pending_credentials_path() -> PathBuf {
        Self::config_dir().join("credentials.pending.yaml")
    }

    pub fn state_dir() -> PathBuf {
        home_dir()
            .map(|h| h.join(".local").join("state").join("mac-k3d"))
            .unwrap_or_else(|| PathBuf::from(".mac-k3d"))
    }

    pub fn load(path: Option<&Path>) -> Result<Self> {
        let path = Self::resolve_config_path(path);

        if !path.exists() {
            return Ok(Self::default());
        }

        Self::load_file(&path)
    }

    /// Load a specific YAML path. Errors if the file is missing or invalid.
    pub fn load_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(Error::Config(format!(
                "config not found: {}",
                path.display()
            )));
        }

        let contents = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("failed to read {}: {e}", path.display())))?;

        serde_yaml::from_str(&contents)
            .map_err(|e| Error::Config(format!("failed to parse {}: {e}", path.display())))
    }

    /// Default dest for `import` when `-c` is omitted: worker.yaml vs config.yaml.
    pub fn default_path_for_role(role: NodeRole) -> PathBuf {
        match role {
            NodeRole::Worker => Self::default_worker_path(),
            NodeRole::Standalone | NodeRole::Controller => Self::default_config_path(),
        }
    }

    /// Lab template: keep eval/job/role fields; drop secrets and host-local paths.
    ///
    /// Never copies `credentials.pending.yaml`.
    pub fn for_export(&self) -> Self {
        let mut out = self.clone();
        out.platform = None;
        out.storage = StorageConfig::default();
        out.lolbench.path = None;
        strip_dependency_host_paths(&mut out.dependencies);
        out.jenkins_agent.remote_fs = None;
        out.jenkins_agent.agent_jar = None;
        out.jenkins_agent.cpu_cores = 0;
        out.jenkins_agent.api_token = None;
        out
    }

    /// Record the OS of the machine that is importing this file.
    pub fn apply_host_platform(&mut self) {
        self.platform = Some(crate::platform::host_os().as_str().to_string());
    }

    pub fn save(&self, path: Option<&Path>) -> Result<()> {
        let path = path
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_config_path);
        write_yaml(&path, self, None)
    }

    /// Write a sanitized export with a header that warns the file has no secrets.
    pub fn save_sanitized_export(&self, path: &Path) -> Result<()> {
        write_yaml(path, &self.for_export(), Some(EXPORT_HEADER))
    }

    pub fn apply_jenkins_mode(&mut self, mode: JenkinsMode) {
        self.jenkins.enabled = matches!(mode, JenkinsMode::InCluster);
        if self.jenkins.enabled {
            self.role = NodeRole::Controller;
        }
    }

    /// Minimum free disk in GB for prepare validation.
    pub fn disk_min_gb(&self) -> u64 {
        if self.resources.disk_min_gb > 0 {
            return self.resources.disk_min_gb;
        }
        match self.role {
            NodeRole::Standalone => 40,
            NodeRole::Controller => 60,
            NodeRole::Worker => 40,
        }
    }
}

impl DependenciesConfig {
    pub fn entries(&self) -> [(&str, &DependencyEntry); 6] {
        [
            ("docker", &self.docker),
            ("k3d", &self.k3d),
            ("kubectl", &self.kubectl),
            ("helm", &self.helm),
            ("harbor", &self.harbor),
            ("java", &self.java),
        ]
    }
}

const EXPORT_HEADER: &str = "\
# mac-k3d sanitized export — no API tokens, CI keys, or host-local paths.
# Never copy credentials.pending.yaml. Edit jenkins_job / labels / controller_url, then:
#   mac-k3d import this-file.yaml
";

fn strip_dependency_host_paths(deps: &mut DependenciesConfig) {
    for entry in [
        &mut deps.docker,
        &mut deps.k3d,
        &mut deps.kubectl,
        &mut deps.helm,
        &mut deps.harbor,
        &mut deps.java,
    ] {
        entry.binary = None;
        entry.app = None;
    }
}

fn write_yaml(path: &Path, config: &MacK3dConfig, header: Option<&str>) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Config(format!("failed to create config dir: {e}")))?;
        }
    }

    let body = serde_yaml::to_string(config)
        .map_err(|e| Error::Config(format!("failed to serialize config: {e}")))?;
    let contents = match header {
        Some(h) => format!("{h}{body}"),
        None => body,
    };

    std::fs::write(path, contents)
        .map_err(|e| Error::Config(format!("failed to write {}: {e}", path.display())))?;

    Ok(())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn resolve_config_path_in(dir: &Path, explicit: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    let primary = dir.join("config.yaml");
    if primary.exists() {
        return primary;
    }
    let worker = dir.join("worker.yaml");
    if worker.exists() {
        return worker;
    }
    primary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefers_config_yaml_when_present() {
        let dir = std::env::temp_dir().join(format!("mac-k3d-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.yaml"), "role: controller\n").unwrap();
        std::fs::write(dir.join("worker.yaml"), "role: worker\n").unwrap();
        assert_eq!(resolve_config_path_in(&dir, None), dir.join("config.yaml"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_falls_back_to_worker_yaml() {
        let dir = std::env::temp_dir().join(format!("mac-k3d-cfg-worker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("worker.yaml"), "role: worker\n").unwrap();
        assert_eq!(resolve_config_path_in(&dir, None), dir.join("worker.yaml"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn sample_worker() -> MacK3dConfig {
        let mut cfg = MacK3dConfig::default();
        cfg.role = NodeRole::Worker;
        cfg.platform = Some("linux".into());
        cfg.storage.base_dir = Some(PathBuf::from("/home/src/mac-k3d"));
        cfg.dependencies.docker.binary = Some(PathBuf::from("/usr/bin/docker"));
        cfg.dependencies.docker.app = Some(PathBuf::from("/Applications/Docker.app"));
        cfg.lolbench.path = Some(PathBuf::from("/home/src/lolbench"));
        cfg.lolbench.source = LolbenchSource::Existing;
        cfg.jenkins_agent.controller_url = Some("http://43.107.42.252:17070".into());
        cfg.jenkins_agent.name = Some("linux-eval-1".into());
        cfg.jenkins_agent.labels = vec!["linux".into(), "docker".into()];
        cfg.jenkins_agent.remote_fs = Some(PathBuf::from("/home/src/jenkins-agent"));
        cfg.jenkins_agent.agent_jar = Some(PathBuf::from("/tmp/agent.jar"));
        cfg.jenkins_agent.cpu_cores = 8;
        cfg.jenkins_agent.api_user = Some("admin".into());
        cfg.jenkins_agent.api_token = Some("test-jenkins-token".into());
        cfg.jenkins_job.default_task = "ruff_1".into();
        cfg.jenkins_job.default_eval_mode = "binary".into();
        cfg
    }

    #[test]
    fn for_export_strips_token_and_host_paths_keeps_job_defaults() {
        let exported = sample_worker().for_export();
        assert_eq!(exported.role, NodeRole::Worker);
        assert!(exported.platform.is_none());
        assert!(exported.storage.base_dir.is_none());
        assert!(exported.dependencies.docker.binary.is_none());
        assert!(exported.dependencies.docker.app.is_none());
        assert!(exported.lolbench.path.is_none());
        assert_eq!(exported.lolbench.source, LolbenchSource::Existing);
        assert_eq!(
            exported.jenkins_agent.controller_url.as_deref(),
            Some("http://43.107.42.252:17070")
        );
        assert_eq!(exported.jenkins_agent.name.as_deref(), Some("linux-eval-1"));
        assert_eq!(exported.jenkins_agent.api_user.as_deref(), Some("admin"));
        assert!(exported.jenkins_agent.api_token.is_none());
        assert!(exported.jenkins_agent.remote_fs.is_none());
        assert!(exported.jenkins_agent.agent_jar.is_none());
        assert_eq!(exported.jenkins_agent.cpu_cores, 0);
        assert_eq!(exported.jenkins_job.default_task, "ruff_1");
        assert_eq!(exported.jenkins_job.default_eval_mode, "binary");
    }

    #[test]
    fn sanitized_export_yaml_omits_token() {
        let dir = std::env::temp_dir().join(format!("mac-k3d-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("worker.yaml");
        sample_worker().save_sanitized_export(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("sanitized export"));
        assert!(!text.contains("test-jenkins-token"));
        assert!(!text.contains("api_token"));
        assert!(text.contains("default_task: ruff_1"));
        let loaded = MacK3dConfig::load_file(&path).unwrap();
        assert!(loaded.jenkins_agent.api_token.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jenkins_job_eval_fields_roundtrip() {
        let mut cfg = MacK3dConfig::default();
        cfg.jenkins_job.default_harness = "icode".into();
        cfg.jenkins_job.default_llm = "deepseek".into();
        cfg.jenkins_job.default_benchmark = "deepswe".into();
        cfg.jenkins_job.default_n_tasks = 2;
        cfg.jenkins_job.default_task.clear();
        cfg.jenkins_job.default_tasks =
            vec!["abs-module-cache-flags".into(), "abs-stepped-slices".into()];
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        assert!(yaml.contains("default_harness: icode"));
        assert!(yaml.contains("default_llm: deepseek"));
        assert!(yaml.contains("default_benchmark: deepswe"));
        assert!(yaml.contains("default_n_tasks: 2"));
        assert!(yaml.contains("abs-module-cache-flags"));
        let loaded: MacK3dConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(loaded.jenkins_job.default_harness, "icode");
        assert_eq!(loaded.jenkins_job.default_llm, "deepseek");
        assert_eq!(loaded.jenkins_job.default_benchmark, "deepswe");
        assert_eq!(loaded.jenkins_job.default_n_tasks, 2);
        assert_eq!(
            loaded.jenkins_job.default_tasks,
            vec!["abs-module-cache-flags", "abs-stepped-slices"]
        );
        assert!(loaded.jenkins_job.default_task.is_empty());

        let omitted: MacK3dConfig = serde_yaml::from_str("role: controller\n").unwrap();
        assert_eq!(omitted.jenkins_job.default_n_tasks, 1);
        assert!(omitted.jenkins_job.default_tasks.is_empty());

        cfg.jenkins_job.default_task = "abs-stepped-slices".into();
        let exported = cfg.for_export();
        assert_eq!(exported.jenkins_job.default_harness, "icode");
        assert_eq!(exported.jenkins_job.default_benchmark, "deepswe");
        assert_eq!(exported.jenkins_job.default_tasks.len(), 2);
    }

    #[test]
    fn default_path_for_role_splits_worker_and_controller() {
        assert_eq!(
            MacK3dConfig::default_path_for_role(NodeRole::Worker),
            MacK3dConfig::default_worker_path()
        );
        assert_eq!(
            MacK3dConfig::default_path_for_role(NodeRole::Controller),
            MacK3dConfig::default_config_path()
        );
        assert_eq!(
            MacK3dConfig::default_path_for_role(NodeRole::Standalone),
            MacK3dConfig::default_config_path()
        );
    }
}
