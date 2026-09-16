//! Allow-lists for eval YAML / Jenkins parameters.
//!
//! v1 runners are `icode` + `deepseek` on `deepswe` | `lolbench`. Append here when a
//! new harness, LLM, or benchmark is implemented.

use crate::error::{Error, Result};

pub const HARNESSES: &[&str] = &["icode"];
pub const LLMS: &[&str] = &["deepseek"];
pub const BENCHMARKS: &[&str] = &["deepswe", "lolbench"];

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
    println!("benchmark:  {}", join_allowed(BENCHMARKS));
    println!("questions:  --task ID  |  --n-tasks N  |  --tasks id,id,…  (mutually exclusive)");
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
            vec!["lolbench", "deepswe"]
        );
    }
}
