//! macOS platform adapter: Docker Desktop, Homebrew, LaunchAgent, /Volumes.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{Error, Result};

const LAUNCH_AGENT_LABEL: &str = "com.mac-k3d.jenkins-agent";

pub fn docker_display_name() -> &'static str {
    "Docker Desktop"
}

pub fn package_manager_label() -> &'static str {
    "Homebrew"
}

pub fn role_prompt() -> &'static str {
    "What is this Mac's role?"
}

pub fn machine_noun() -> &'static str {
    "Mac"
}

pub fn default_agent_labels() -> Vec<String> {
    vec!["macos".into(), "docker".into(), "lolbench".into()]
}

pub fn agent_daemon_label() -> &'static str {
    LAUNCH_AGENT_LABEL
}

pub fn scan_extra_mount_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/Volumes") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                roots.push(path);
            }
        }
    }
    roots
}

pub fn docker_app_default() -> Option<PathBuf> {
    Some(PathBuf::from("/Applications/Docker.app"))
}

pub fn requires_docker_app() -> bool {
    true
}

pub fn agent_path_extras() -> Vec<&'static str> {
    vec![
        "/usr/local/bin",
        "/opt/homebrew/bin",
        "/Applications/Docker.app/Contents/Resources/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ]
}

pub fn preferred_shell_rc_default(home: &Path) -> PathBuf {
    home.join(".zshrc")
}

pub fn package_install_available() -> bool {
    which("brew").is_some()
}

pub fn docker_not_ready_hint() -> &'static str {
    "Open Docker Desktop and wait until the menu-bar whale is idle, then re-run `mac-k3d setup`. \
     Download: https://www.docker.com/products/docker-desktop/"
}

fn prepend_brew_paths() {
    let extra = ["/opt/homebrew/bin", "/usr/local/bin"];
    let old = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<String> = extra.iter().map(|s| (*s).to_string()).collect();
    if !old.is_empty() {
        parts.push(old);
    }
    std::env::set_var("PATH", parts.join(":"));
}

fn ensure_homebrew() -> Result<()> {
    if package_install_available() {
        return Ok(());
    }
    println!(
        "Homebrew not found (needed to install Docker Desktop from this binary).\n\
         https://brew.sh\n\
         Or install Docker Desktop: https://www.docker.com/products/docker-desktop/"
    );
    if !atty::is(atty::Stream::Stdin) {
        return Err(Error::DependencyMissing(
            "Homebrew not found; install from https://brew.sh then re-run mac-k3d setup".into(),
        ));
    }
    let ok = dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt("Run the official Homebrew install script now?")
        .default(true)
        .interact()
        .map_err(|_| Error::Cancelled)?;
    if !ok {
        return Err(Error::DependencyMissing(
            "Homebrew not found; install from https://brew.sh or Docker Desktop from docker.com"
                .into(),
        ));
    }
    println!("Installing Homebrew (may ask for your password)…");
    run_shell(
        r#"NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)""#,
    )?;
    prepend_brew_paths();
    if !package_install_available() {
        return Err(Error::DependencyMissing(
            "Homebrew install finished but `brew` is not on PATH; open a new terminal and re-run mac-k3d setup"
                .into(),
        ));
    }
    Ok(())
}

pub fn install_package(name: &str) -> Result<()> {
    if name == "docker" {
        ensure_homebrew()?;
    } else if !package_install_available() {
        return Err(Error::DependencyMissing(format!(
            "Homebrew not found; install {name} from https://brew.sh"
        )));
    }
    let args: Vec<&str> = match name {
        "java" => vec!["install", "--cask", "temurin"],
        "docker" => vec!["install", "--cask", "docker"],
        "k3d" => vec!["install", "k3d"],
        "kubectl" => vec!["install", "kubectl"],
        "helm" => vec!["install", "helm"],
        "uv" => vec!["install", "uv"],
        "git" => vec!["install", "git"],
        other => {
            return Err(Error::Config(format!(
                "unknown dependency for Homebrew install: {other}"
            )));
        }
    };
    tracing::info!(dependency = name, "installing via Homebrew");
    if let Err(e) = run_cmd("brew", &args) {
        if name == "docker" {
            return Err(Error::DependencyMissing(
                "Homebrew could not install Docker Desktop. Download: https://www.docker.com/products/docker-desktop/"
                    .into(),
            ));
        }
        return Err(e);
    }
    if name == "docker" {
        println!("Opening Docker Desktop…");
        let _ = Command::new("open").args(["-a", "Docker"]).status();
        println!("{}\n", docker_not_ready_hint());
    }
    Ok(())
}

pub fn harbor_bootstrap_hint() -> &'static str {
    "neither uv nor pipx found; install will try Homebrew → uv first"
}

fn which(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.exists().then_some(candidate)
    })
}

fn plist_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
}

fn gui_domain() -> String {
    let uid = Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(501);
    format!("gui/{uid}")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn install_agent_daemon(
    launch_script: &Path,
    working_dir: &Path,
    path_env: &str,
) -> Result<()> {
    if !launch_script.exists() {
        return Err(Error::Config(format!(
            "launch script not found: {}",
            launch_script.display()
        )));
    }

    let body = std::fs::read_to_string(launch_script).map_err(|e| {
        Error::Config(format!("failed to read {}: {e}", launch_script.display()))
    })?;
    if body.contains("REPLACE_ME") {
        println!(
            "Launch script still has REPLACE_ME secret — not starting LaunchAgent.\n\
             Re-run config with a valid API token, or paste the secret, then run config again."
        );
        return Ok(());
    }

    let plist = plist_path();
    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Error::Config(format!("failed to create {}: {e}", parent.display()))
        })?;
    }

    let log_out = working_dir.join("jenkins-agent.stdout.log");
    let log_err = working_dir.join("jenkins-agent.stderr.log");
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/bash</string>
    <string>{script}</string>
  </array>
  <key>WorkingDirectory</key>
  <string>{cwd}</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>{path}</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{stdout}</string>
  <key>StandardErrorPath</key>
  <string>{stderr}</string>
</dict>
</plist>
"#,
        label = LAUNCH_AGENT_LABEL,
        script = xml_escape(&launch_script.display().to_string()),
        cwd = xml_escape(&working_dir.display().to_string()),
        path = xml_escape(path_env),
        stdout = xml_escape(&log_out.display().to_string()),
        stderr = xml_escape(&log_err.display().to_string()),
    );

    std::fs::write(&plist, xml).map_err(|e| {
        Error::Config(format!("failed to write {}: {e}", plist.display()))
    })?;

    let _ = bootout();
    bootstrap(&plist)?;
    println!(
        "Jenkins agent LaunchAgent started ({LAUNCH_AGENT_LABEL}).\n\
         Logs: {}\n\
         Survives this shell; restarts if the process exits. Stop with teardown/clean.",
        log_out.display()
    );
    Ok(())
}

pub fn stop_agent_daemon() -> Result<()> {
    let plist = plist_path();
    let _ = bootout();
    if plist.exists() {
        std::fs::remove_file(&plist).map_err(|e| {
            Error::Config(format!("failed to remove {}: {e}", plist.display()))
        })?;
        println!("Removed LaunchAgent {}", plist.display());
    } else {
        println!("Jenkins agent LaunchAgent not installed; nothing to stop.");
    }
    Ok(())
}

fn bootstrap(plist: &Path) -> Result<()> {
    let domain = gui_domain();
    let status = Command::new("launchctl")
        .args(["bootstrap", &domain, &plist.display().to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| Error::CommandFailed {
            cmd: "launchctl bootstrap".into(),
            source: e.into(),
        })?;
    if status.success() {
        return Ok(());
    }

    let service = format!("{domain}/{LAUNCH_AGENT_LABEL}");
    let kick = Command::new("launchctl")
        .args(["kickstart", "-k", &service])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| Error::CommandFailed {
            cmd: "launchctl kickstart".into(),
            source: e.into(),
        })?;
    if kick.success() {
        return Ok(());
    }

    let status = Command::new("launchctl")
        .args(["load", "-w", &plist.display().to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| Error::CommandFailed {
            cmd: "launchctl load".into(),
            source: e.into(),
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::CommandFailed {
            cmd: "launchctl bootstrap/kickstart/load".into(),
            source: anyhow::anyhow!("exit {:?}", status.code()),
        })
    }
}

fn bootout() -> Result<()> {
    let domain = gui_domain();
    let service = format!("{domain}/{LAUNCH_AGENT_LABEL}");
    let _ = Command::new("launchctl")
        .args(["bootout", &service])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let plist = plist_path();
    if plist.exists() {
        let _ = Command::new("launchctl")
            .args(["unload", "-w", &plist.display().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    Ok(())
}

fn run_cmd(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program).args(args).status().map_err(|e| {
        Error::CommandFailed {
            cmd: format!("{program} {}", args.join(" ")),
            source: e.into(),
        }
    })?;
    if !status.success() {
        return Err(Error::CommandFailed {
            cmd: format!("{program} {}", args.join(" ")),
            source: anyhow::anyhow!("exit code {:?}", status.code()),
        });
    }
    Ok(())
}

fn run_shell(script: &str) -> Result<()> {
    let status = Command::new("bash")
        .args(["-lc", script])
        .status()
        .map_err(|e| Error::CommandFailed {
            cmd: "bash -lc (brew install)".into(),
            source: e.into(),
        })?;
    if !status.success() {
        return Err(Error::CommandFailed {
            cmd: "bash -lc (brew install)".into(),
            source: anyhow::anyhow!("exit code {:?}", status.code()),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_include_macos() {
        assert!(default_agent_labels().contains(&"macos".into()));
    }

    #[test]
    fn plist_path_ends_with_label() {
        assert!(plist_path()
            .to_string_lossy()
            .contains("com.mac-k3d.jenkins-agent.plist"));
    }

    #[test]
    fn docker_hint_mentions_desktop() {
        let h = docker_not_ready_hint();
        assert!(h.contains("Docker Desktop"), "{h}");
        assert!(h.contains("mac-k3d setup"), "{h}");
    }
}
