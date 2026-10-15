//! What a DeepSWE eval needs from this worker's Docker beyond `docker info`:
//! linux/amd64 containers on an arm64 host (every question without a proven
//! native build runs the published amd64 image), and the folders the harness
//! builds mount at their own paths. `config` warns; the env phase and the
//! Prepare harness stage fail on the same problems.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The probe image the pipeline's `ensure_platform_runs` uses.
const PROBE_IMAGE: &str = "alpine:3.20";

pub fn worker_docker_problems(docker: &Path, mounts: &[PathBuf]) -> Vec<String> {
    let Some(arch) = output(docker, &["info", "--format", "{{.Architecture}}"]) else {
        return vec![format!(
            "{} is not answering; skipped the emulation and file-sharing checks",
            crate::platform::docker_display_name()
        )];
    };
    let macos = cfg!(target_os = "macos");
    let mut out = Vec::new();
    if is_arm64(&arch) {
        let got = output(
            docker,
            &["run", "--rm", "--platform", "linux/amd64", PROBE_IMAGE, "uname", "-m"],
        );
        out.extend(amd64_problem(got.as_deref(), macos));
        if macos && got.as_deref() == Some("x86_64") && rosetta_off() {
            out.push(
                "Docker Desktop runs linux/amd64 with QEMU, not Rosetta: slower and less stable. Turn on \
                 Docker Desktop > Settings > General > \"Use Rosetta for x86_64/amd64 emulation on Apple Silicon\""
                    .into(),
            );
        }
    }
    for dir in mounts.iter().filter(|d| d.is_dir()) {
        let spec = format!("{0}:{0}:ro", dir.display());
        let ok = output(docker, &["run", "--rm", "-v", &spec, PROBE_IMAGE, "test", "-d", &dir.to_string_lossy()])
            .is_some();
        out.extend(mount_problem(dir, ok, macos));
    }
    out
}

fn is_arm64(arch: &str) -> bool {
    matches!(arch, "aarch64" | "arm64")
}

/// Trimmed stdout of a command that succeeded.
fn output(docker: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new(docker)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn amd64_problem(uname: Option<&str>, macos: bool) -> Option<String> {
    if uname == Some("x86_64") {
        return None;
    }
    let got = uname.filter(|u| !u.is_empty()).unwrap_or("nothing");
    let fix = if macos {
        "Docker Desktop > Settings > General > \"Use Rosetta for x86_64/amd64 emulation on Apple Silicon\", Apply & restart"
    } else {
        "docker run --privileged --rm tonistiigi/binfmt --install amd64"
    };
    Some(format!(
        "Docker cannot run linux/amd64 containers (uname -m gave '{got}'): DeepSWE questions without a \
         proven native build fail. Fix: {fix}"
    ))
}

fn mount_problem(dir: &Path, ok: bool, macos: bool) -> Option<String> {
    if ok {
        return None;
    }
    let fix = if macos {
        "add it (or a parent) under Docker Desktop > Settings > Resources > File sharing"
    } else {
        "check that the Docker daemon runs on this machine"
    };
    Some(format!(
        "Docker cannot mount {}: the iCode and JiuwenSwarm builds mount it at the same path; {fix}",
        dir.display()
    ))
}

/// Docker Desktop's settings turn Rosetta off (the pipeline reads the same keys).
fn rosetta_off() -> bool {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return false;
    };
    let group = home.join("Library/Group Containers/group.com.docker");
    ["settings-store.json", "settings.json"]
        .iter()
        .filter_map(|name| std::fs::read_to_string(group.join(name)).ok())
        .any(|text| settings_turn_rosetta_off(&text))
}

fn settings_turn_rosetta_off(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|doc| doc.get("UseVirtualizationFrameworkRosetta").cloned())
        == Some(serde_json::Value::Bool(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amd64_is_fine_when_the_probe_says_x86_64() {
        assert_eq!(amd64_problem(Some("x86_64"), true), None);
        let mac = amd64_problem(Some("aarch64"), true).unwrap();
        assert!(mac.contains("uname -m gave 'aarch64'"), "{mac}");
        assert!(mac.contains("Use Rosetta"), "{mac}");
        let linux = amd64_problem(None, false).unwrap();
        assert!(linux.contains("gave 'nothing'"), "{linux}");
        assert!(linux.contains("tonistiigi/binfmt --install amd64"), "{linux}");
    }

    #[test]
    fn an_unshared_folder_names_file_sharing_on_a_mac() {
        let dir = Path::new("/Volumes/Data/jenkins-agent");
        assert_eq!(mount_problem(dir, true, true), None);
        let mac = mount_problem(dir, false, true).unwrap();
        assert!(mac.contains("cannot mount /Volumes/Data/jenkins-agent"), "{mac}");
        assert!(mac.contains("Resources > File sharing"), "{mac}");
        assert!(!mount_problem(dir, false, false).unwrap().contains("File sharing"));
    }

    #[test]
    fn rosetta_is_off_only_when_the_setting_says_false() {
        assert!(settings_turn_rosetta_off(r#"{"UseVirtualizationFrameworkRosetta": false}"#));
        assert!(!settings_turn_rosetta_off(r#"{"UseVirtualizationFrameworkRosetta": true}"#));
        assert!(!settings_turn_rosetta_off(r#"{"memoryMiB": 8192}"#));
        assert!(!settings_turn_rosetta_off("not json"));
    }

    #[test]
    fn a_missing_docker_skips_the_checks_with_one_warning() {
        let problems = worker_docker_problems(Path::new("/nonexistent/docker"), &[PathBuf::from("/")]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("not answering"), "{problems:?}");
    }
}
