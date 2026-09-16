use std::path::Path;

use clap::Args;
use dialoguer::{theme::ColorfulTheme, Input, Select};

use crate::config::MacK3dConfig;
use crate::error::{Error, Result};
use crate::eval_catalog;

#[derive(Debug, Args)]
pub struct SetArgs {
    /// Print allowed harness / LLM / benchmark values and exit
    #[arg(long)]
    pub list: bool,

    /// Harness id (catalog)
    #[arg(long)]
    pub harness: Option<String>,

    /// LLM family (catalog)
    #[arg(long)]
    pub llm: Option<String>,

    /// Benchmark (`deepswe` or `lolbench`)
    #[arg(long)]
    pub benchmark: Option<String>,

    /// One question id
    #[arg(long)]
    pub task: Option<String>,

    /// First N sorted questions (clears TASK / TASKS)
    #[arg(long)]
    pub n_tasks: Option<u32>,

    /// Comma-separated question ids
    #[arg(long)]
    pub tasks: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct SetPatch {
    pub harness: Option<String>,
    pub llm: Option<String>,
    pub benchmark: Option<String>,
    pub task: Option<String>,
    pub n_tasks: Option<u32>,
    pub tasks: Option<String>,
}

/// Edit `jenkins_job` on a YAML file. Does not start Jenkins or upload secrets.
pub fn run(args: SetArgs, config_path: Option<&Path>) -> Result<()> {
    if args.list {
        eval_catalog::print_catalog();
        return Ok(());
    }

    let question_modes = usize::from(args.task.is_some())
        + usize::from(args.n_tasks.is_some())
        + usize::from(args.tasks.is_some());
    if question_modes > 1 {
        return Err(Error::Config(
            "--task, --n-tasks, and --tasks are mutually exclusive".into(),
        ));
    }

    let path = MacK3dConfig::resolve_config_path(config_path);
    let mut config = MacK3dConfig::load_file(&path)?;

    let mut patch = SetPatch {
        harness: args.harness,
        llm: args.llm,
        benchmark: args.benchmark,
        task: args.task,
        n_tasks: args.n_tasks,
        tasks: args.tasks,
    };

    let any_flag = patch.harness.is_some()
        || patch.llm.is_some()
        || patch.benchmark.is_some()
        || patch.task.is_some()
        || patch.n_tasks.is_some()
        || patch.tasks.is_some();

    if !any_flag {
        if atty::is(atty::Stream::Stdin) {
            patch = prompt_patch(&config)?;
        } else {
            return Err(Error::Config(
                "not a TTY: pass --harness / --llm / --benchmark / --task / --n-tasks / --tasks, or --list"
                    .into(),
            ));
        }
    }

    apply_set(&mut config, patch)?;
    config.save(Some(&path))?;
    print_set_summary(&config, &path);
    Ok(())
}

pub fn apply_set(config: &mut MacK3dConfig, patch: SetPatch) -> Result<()> {
    let job = &mut config.jenkins_job;
    if let Some(h) = patch.harness {
        job.default_harness = eval_catalog::require_harness(&h)?;
    }
    if let Some(l) = patch.llm {
        job.default_llm = eval_catalog::require_llm(&l)?;
    }
    if let Some(b) = patch.benchmark {
        job.default_benchmark = eval_catalog::require_benchmark(&b)?;
    }

    let q = usize::from(patch.task.is_some())
        + usize::from(patch.n_tasks.is_some())
        + usize::from(patch.tasks.is_some());
    if q > 1 {
        return Err(Error::Config(
            "--task, --n-tasks, and --tasks are mutually exclusive".into(),
        ));
    }

    if let Some(task) = patch.task {
        let t = task.trim();
        if t.is_empty() {
            return Err(Error::Config("--task must not be empty".into()));
        }
        if t.contains(',') {
            return Err(Error::Config(
                "for several ids use --tasks a,b (not --task)".into(),
            ));
        }
        job.default_task = t.to_string();
        job.default_tasks.clear();
        job.default_n_tasks = 1;
    } else if let Some(n) = patch.n_tasks {
        if n == 0 {
            return Err(Error::Config("--n-tasks must be >= 1".into()));
        }
        job.default_task.clear();
        job.default_tasks.clear();
        job.default_n_tasks = n;
    } else if let Some(raw) = patch.tasks {
        let ids = eval_catalog::parse_task_list(&raw);
        if ids.is_empty() {
            return Err(Error::Config("--tasks must list at least one id".into()));
        }
        job.default_tasks = ids;
        job.default_n_tasks = job.default_tasks.len() as u32;
        job.default_task.clear();
    }
    Ok(())
}

fn prompt_patch(config: &MacK3dConfig) -> Result<SetPatch> {
    let theme = ColorfulTheme::default();
    let h_idx = index_of(
        eval_catalog::HARNESSES,
        nonempty_or(&config.jenkins_job.default_harness, "icode"),
    );
    let l_idx = index_of(
        eval_catalog::LLMS,
        nonempty_or(
            &config.jenkins_job.default_llm,
            nonempty_or(&config.jenkins_job.default_model, "deepseek"),
        ),
    );
    let b_idx = index_of(
        eval_catalog::BENCHMARKS,
        nonempty_or(&config.jenkins_job.default_benchmark, "deepswe"),
    );

    let harness = eval_catalog::HARNESSES[Select::with_theme(&theme)
        .with_prompt("Harness")
        .items(eval_catalog::HARNESSES)
        .default(h_idx)
        .interact()
        .map_err(|_| Error::Cancelled)?]
    .to_string();
    let llm = eval_catalog::LLMS[Select::with_theme(&theme)
        .with_prompt("LLM")
        .items(eval_catalog::LLMS)
        .default(l_idx)
        .interact()
        .map_err(|_| Error::Cancelled)?]
    .to_string();
    let benchmark = eval_catalog::BENCHMARKS[Select::with_theme(&theme)
        .with_prompt("Benchmark")
        .items(eval_catalog::BENCHMARKS)
        .default(b_idx)
        .interact()
        .map_err(|_| Error::Cancelled)?]
    .to_string();

    let mode = Select::with_theme(&theme)
        .with_prompt("Questions")
        .items(&[
            "one specific id (--task)",
            "first N sorted (--n-tasks)",
            "explicit list (--tasks)",
        ])
        .default(0)
        .interact()
        .map_err(|_| Error::Cancelled)?;

    let mut patch = SetPatch {
        harness: Some(harness),
        llm: Some(llm),
        benchmark: Some(benchmark),
        ..SetPatch::default()
    };
    match mode {
        0 => {
            patch.task = Some(
                Input::with_theme(&theme)
                    .with_prompt("TASK id")
                    .default(config.jenkins_job.default_task.clone())
                    .interact_text()
                    .map_err(|_| Error::Cancelled)?,
            );
        }
        1 => {
            let n = config.jenkins_job.default_n_tasks.max(1);
            patch.n_tasks = Some(
                Input::with_theme(&theme)
                    .with_prompt("N_TASKS (first N sorted; paid if N>1)")
                    .default(n)
                    .interact_text()
                    .map_err(|_| Error::Cancelled)?,
            );
        }
        _ => {
            let default = if config.jenkins_job.default_tasks.is_empty() {
                config.jenkins_job.default_task.clone()
            } else {
                config.jenkins_job.default_tasks.join(",")
            };
            patch.tasks = Some(
                Input::with_theme(&theme)
                    .with_prompt("TASKS (comma-separated ids)")
                    .default(default)
                    .interact_text()
                    .map_err(|_| Error::Cancelled)?,
            );
        }
    }
    Ok(patch)
}

fn nonempty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim()
    }
}

fn index_of(items: &[&str], value: &str) -> usize {
    items.iter().position(|a| *a == value).unwrap_or(0)
}

fn print_set_summary(config: &MacK3dConfig, path: &Path) {
    let job = &config.jenkins_job;
    println!("Updated {} (YAML only; secrets unchanged).", path.display());
    println!(
        "  harness={}  llm={}  benchmark={}",
        nonempty_or(&job.default_harness, "(unset)"),
        nonempty_or(&job.default_llm, nonempty_or(&job.default_model, "(unset)")),
        nonempty_or(&job.default_benchmark, "(unset)")
    );
    if !job.default_tasks.is_empty() {
        println!("  tasks={}", job.default_tasks.join(","));
    } else if !job.default_task.is_empty() {
        println!("  task={}", job.default_task);
    } else {
        println!("  n_tasks={} (first N sorted)", job.default_n_tasks.max(1));
    }
    println!(
        "Controller: mac-k3d config -c {} --skip-secrets",
        path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::JenkinsJobConfig;

    fn cfg() -> MacK3dConfig {
        MacK3dConfig {
            jenkins_job: JenkinsJobConfig::default(),
            ..MacK3dConfig::default()
        }
    }

    #[test]
    fn set_task_clears_list_and_n() {
        let mut c = cfg();
        c.jenkins_job.default_tasks = vec!["a".into(), "b".into()];
        c.jenkins_job.default_n_tasks = 5;
        apply_set(
            &mut c,
            SetPatch {
                task: Some("abs-stepped-slices".into()),
                ..SetPatch::default()
            },
        )
        .unwrap();
        assert_eq!(c.jenkins_job.default_task, "abs-stepped-slices");
        assert!(c.jenkins_job.default_tasks.is_empty());
        assert_eq!(c.jenkins_job.default_n_tasks, 1);
    }

    #[test]
    fn set_n_tasks_clears_task() {
        let mut c = cfg();
        apply_set(
            &mut c,
            SetPatch {
                n_tasks: Some(2),
                benchmark: Some("deepswe".into()),
                ..SetPatch::default()
            },
        )
        .unwrap();
        assert!(c.jenkins_job.default_task.is_empty());
        assert_eq!(c.jenkins_job.default_n_tasks, 2);
        assert_eq!(c.jenkins_job.default_benchmark, "deepswe");
    }

    #[test]
    fn set_tasks_list() {
        let mut c = cfg();
        apply_set(
            &mut c,
            SetPatch {
                tasks: Some("abs-module-cache-flags, abs-stepped-slices".into()),
                ..SetPatch::default()
            },
        )
        .unwrap();
        assert_eq!(
            c.jenkins_job.default_tasks,
            vec!["abs-module-cache-flags", "abs-stepped-slices"]
        );
        assert!(c.jenkins_job.default_task.is_empty());
        assert_eq!(c.jenkins_job.default_n_tasks, 2);
    }

    #[test]
    fn exclusive_question_modes() {
        let mut c = cfg();
        let err = apply_set(
            &mut c,
            SetPatch {
                task: Some("a".into()),
                n_tasks: Some(2),
                ..SetPatch::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("mutually exclusive"), "{err}");
    }

    #[test]
    fn reject_unknown_llm() {
        let mut c = cfg();
        let err = apply_set(
            &mut c,
            SetPatch {
                llm: Some("gpt".into()),
                ..SetPatch::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("allowed: deepseek"), "{err}");
    }
}
