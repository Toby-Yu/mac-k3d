//! Embed `pipeline/` in the Release binary and extract it to the user share dir.

use std::path::{Path, PathBuf};

use include_dir::{include_dir, Dir};

use crate::error::{Error, Result};

static PIPELINE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/pipeline");

const RUN_ALL: &str = "pipeline/stages/run_all.sh";

pub fn run_all_rel() -> &'static str {
    RUN_ALL
}

pub fn looks_like_root(path: &Path) -> bool {
    path.join(RUN_ALL).is_file()
}

pub fn share_dir() -> PathBuf {
    if let Ok(p) = std::env::var("MAC_K3D_SHARE") {
        let t = p.trim();
        if !t.is_empty() {
            return PathBuf::from(t);
        }
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        let t = xdg.trim();
        if !t.is_empty() {
            return PathBuf::from(t).join("mac-k3d");
        }
    }
    home.join(".local/share/mac-k3d")
}

pub fn icode_drop_hint(share: &Path) -> String {
    format!(
        "Place icode or icode-<os>-<arch>-full-vX.Y.Z (.tar.gz, same name, or unpacked folder) in {} (or /opt/mac-k3d/)",
        share.display()
    )
}

fn dir_has_named_icode(dir: &Path) -> bool {
    fn walk(d: &Path, depth: u32) -> bool {
        if depth > 6 {
            return false;
        }
        let Ok(rd) = std::fs::read_dir(d) else {
            return false;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() && p.file_name().and_then(|s| s.to_str()) == Some("icode") {
                return true;
            }
            if p.is_dir() && walk(&p, depth + 1) {
                return true;
            }
        }
        false
    }
    dir.join("icode").is_file() || walk(dir, 0)
}

/// File named `icode`, archive, `*-full-*` file, or `*-full-*` directory with `icode` inside.
pub fn looks_like_icode_release(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if path.is_dir() {
        return name.contains("-full-") && dir_has_named_icode(path);
    }
    if !path.is_file() {
        return false;
    }
    name == "icode"
        || name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.contains("-full-")
}

fn first_full_drop(dir: &Path) -> Option<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return None;
    };
    let mut found: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "icode" || name == "pipeline" {
                return false;
            }
            looks_like_icode_release(p)
                && (name.contains("-full-") || name.ends_with(".tar.gz") || name.ends_with(".tgz"))
        })
        .collect();
    found.sort();
    found.into_iter().next()
}

/// Search one directory: official `*-full-*` / archives first, then a file named `icode`.
pub fn discover_icode_release_in(dir: &Path) -> Option<PathBuf> {
    if let Some(p) = first_full_drop(dir) {
        return Some(p);
    }
    let named = dir.join("icode");
    if looks_like_icode_release(&named) {
        return Some(named);
    }
    None
}

/// Share dir first (`MAC_K3D_SHARE`), then `/opt/mac-k3d`.
pub fn discover_icode_release() -> Option<PathBuf> {
    let share = share_dir();
    if let Some(p) = discover_icode_release_in(&share) {
        return Some(p);
    }
    discover_icode_release_in(Path::new("/opt/mac-k3d"))
}

/// Extract embedded pipeline/ into `root/pipeline/` without touching `root/icode`.
pub fn extract_pipeline(root: &Path) -> Result<()> {
    let dest = root.join("pipeline");
    std::fs::create_dir_all(&dest)
        .map_err(|e| Error::Config(format!("cannot create {}: {e}", dest.display())))?;
    PIPELINE.extract(&dest).map_err(|e| {
        Error::Config(format!(
            "failed to extract pipeline assets to {}: {e}",
            dest.display()
        ))
    })?;
    Ok(())
}

pub fn ensure_share_pipeline() -> Result<PathBuf> {
    let share = share_dir();
    std::fs::create_dir_all(&share)
        .map_err(|e| Error::Config(format!("cannot create {}: {e}", share.display())))?;
    extract_pipeline(&share)?;
    if !looks_like_root(&share) {
        return Err(Error::Config(format!(
            "pipeline extract missing {RUN_ALL} under {}",
            share.display()
        )));
    }
    Ok(share)
}

fn icode_drop_present(share: &Path) -> bool {
    discover_icode_release_in(share).is_some()
        || discover_icode_release_in(Path::new("/opt/mac-k3d")).is_some()
}

/// Extract `pipeline/` into the share dir and print the path (plus an iCode drop hint if missing).
/// Never writes into `share/icode`.
pub fn ensure_share_pipeline_reported() -> Result<PathBuf> {
    let share = ensure_share_pipeline()?;
    let pipeline = share.join("pipeline");
    if icode_drop_present(&share) {
        println!("Extracted pipeline to {}", pipeline.display());
    } else {
        println!(
            "Extracted pipeline to {}. {}",
            pipeline.display(),
            icode_drop_hint(&share)
        );
    }
    Ok(share)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_pipeline_has_run_all() {
        assert!(
            PIPELINE.get_file("stages/run_all.sh").is_some(),
            "pipeline/stages/run_all.sh must be embedded"
        );
        assert!(PIPELINE.get_file("lib/icode_pier_agent.py").is_some());
        assert!(
            PIPELINE.get_file("lib/openai_compat.py").is_some(),
            "pipeline/lib/openai_compat.py must be embedded"
        );
    }

    #[test]
    fn openai_compat_parses_list_json_without_network() {
        let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pipeline/lib");
        let output = std::process::Command::new("python3")
            .current_dir(&lib)
            .args([
                "-c",
                "from openai_compat import parse_model_ids, model_in_ids, missing_model_message\n\
ids = parse_model_ids({'object':'list','data':[{'id':'deepseek-flash'},{'id':'deepseek-v4-pro'}]})\n\
assert ids == ['deepseek-flash', 'deepseek-v4-pro']\n\
assert model_in_ids('deepseek-flash', ids)\n\
assert not model_in_ids('deepseek-v4.1-flash', ids)\n\
msg = missing_model_message('deepseek-v4.1-flash', ids)\n\
assert \"is not returned by GET /models\" in msg\n\
print('ok')\n",
            ])
            .output()
            .expect("python3");
        assert!(
            output.status.success(),
            "stderr={} stdout={}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
    }

    #[test]
    fn extract_pipeline_does_not_touch_icode() {
        let root =
            std::env::temp_dir().join(format!("mac-k3d-extract-icode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let icode = root.join("icode");
        std::fs::write(&icode, b"keep-me").unwrap();
        extract_pipeline(&root).unwrap();
        assert_eq!(std::fs::read(&icode).unwrap(), b"keep-me");
        assert!(looks_like_root(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn share_dir_honors_mac_k3d_share() {
        let prev = std::env::var_os("MAC_K3D_SHARE");
        std::env::set_var("MAC_K3D_SHARE", "/tmp/mac-k3d-share-test");
        let got = share_dir();
        match prev {
            Some(v) => std::env::set_var("MAC_K3D_SHARE", v),
            None => std::env::remove_var("MAC_K3D_SHARE"),
        }
        assert_eq!(got, PathBuf::from("/tmp/mac-k3d-share-test"));
    }

    #[test]
    fn discover_official_full_release_in_share() {
        let dir = std::env::temp_dir().join(format!("mac-k3d-full-drop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tar = dir.join("icode-linux-x86_64-full-v0.1.41.tar.gz");
        std::fs::write(&tar, b"not-a-real-archive").unwrap();
        let got = discover_icode_release_in(&dir).expect("find *-full-*.tar.gz");
        assert_eq!(got, tar);

        let bare_dir = dir.join("icode-linux-x86_64-full-v0.1.41");
        std::fs::remove_file(&tar).unwrap();
        std::fs::create_dir_all(&bare_dir).unwrap();
        std::fs::write(bare_dir.join("icode"), b"#!/bin/sh\n").unwrap();
        let got = discover_icode_release_in(&dir).expect("find unpacked *-full-* dir");
        assert_eq!(got, bare_dir);

        std::fs::remove_dir_all(&bare_dir).unwrap();
        std::fs::write(dir.join("notes.txt"), b"nope").unwrap();
        assert!(discover_icode_release_in(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn looks_like_official_full_name_without_suffix() {
        let dir = std::env::temp_dir().join(format!("mac-k3d-full-bare-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bare = dir.join("icode-linux-x86_64-full-v0.1.41");
        std::fs::write(&bare, b"\x1f\x8bgzip-stub").unwrap();
        assert!(looks_like_icode_release(&bare));
        assert!(!looks_like_icode_release(&dir.join("notes.txt")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_prefers_full_release_over_named_icode() {
        let dir =
            std::env::temp_dir().join(format!("mac-k3d-full-vs-icode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("icode"), b"#!/bin/sh\n# leftover wrapper\n").unwrap();
        let tar = dir.join("icode-linux-x86_64-full-v0.1.41.tar.gz");
        std::fs::write(&tar, b"stub").unwrap();
        let got = discover_icode_release_in(&dir).expect("prefer *-full-*");
        assert_eq!(got, tar);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
