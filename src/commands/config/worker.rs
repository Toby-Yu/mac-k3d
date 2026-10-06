//! Worker `config`: check the eval toolchain, then register the Jenkins agent.
//! A worker hosts no cluster, so nothing here touches k3d or kubectl.

use std::path::Path;

use super::ConfigArgs;
use crate::config::MacK3dConfig;
use crate::error::Result;
use crate::prepare::{discovery, jenkins_agent, toolchain, wizard};

pub fn run(args: &ConfigArgs, config: &MacK3dConfig, config_path: &Path) -> Result<()> {
    check_toolchain(config_path);
    if args.skip_agent {
        return Ok(());
    }
    if config.jenkins_agent.api_credentials().is_none() {
        println!("{}", wizard::blank_api_keys_hint(config_path));
    }
    println!("Ensuring Jenkins agent registration…");
    jenkins_agent::ensure_worker_agent(config)
}

/// Warn only: the env phase of every build fails on a wrong Harbor anyway, and
/// the agent should still come up so that failure is visible in Jenkins.
fn check_toolchain(config_path: &Path) {
    let fix = format!(
        "run `mac-k3d setup -c {}` and choose \"Use existing config\"",
        config_path.display()
    );
    let want = toolchain::harbor_version();
    match toolchain::installed_harbor_version() {
        Some(got) if got == want => println!("harbor {want}: ok"),
        Some(got) => println!("Warning: harbor {got} is installed but builds need {want}; {fix}."),
        None => println!("Warning: harbor not found (builds need {want}); {fix}."),
    }
    if discovery::which("git").is_none() {
        println!("Warning: git not found; the tasks phase clones benchmarks with it; {fix}.");
    }
}
