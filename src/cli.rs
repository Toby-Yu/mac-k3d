use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::commands;

#[derive(Debug, Parser)]
#[command(
    name = "mac-k3d",
    version,
    about = "Manage k3d + Docker on macOS and Linux (optional Jenkins)",
    long_about = None
)]
pub struct Cli {
    /// Path to configuration file (default: ~/.config/mac-k3d/config.yaml)
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Increase logging verbosity (-v, -vv)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Interactive first-run: wizard, then start (controller) or config (worker)
    Setup(commands::SetupArgs),

    /// Verify prerequisites and prepare the local environment
    Prepare(commands::PrepareArgs),

    /// Start Docker, k3d cluster, and optional Jenkins
    Start(commands::StartArgs),

    /// Apply configuration (kubeconfig, port-forwards, Jenkins setup)
    Config(commands::ConfigArgs),

    /// Run iCode / DeepSeek eval (DeepSWE or LoLBench) locally or via Jenkins *_one_task jobs
    Eval(commands::EvalArgs),

    /// Stop cluster and services without removing data
    Teardown(commands::TeardownArgs),

    /// Remove cluster, volumes, and local state
    Clean(commands::CleanArgs),

    /// Show current environment status
    Status,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum JenkinsMode {
  #[default]
  /// Do not install or manage Jenkins
  Skip,
  /// Deploy Jenkins into the k3d cluster
  InCluster,
}
