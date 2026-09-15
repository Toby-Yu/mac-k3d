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
        "Place the iCode binary or *-full-*.tar.gz at {}/icode (or /opt/mac-k3d/icode)",
        share.display()
    )
}

/// Extract embedded pipeline/ into `root/pipeline/` without touching `root/icode`.
pub fn extract_pipeline(root: &Path) -> Result<()> {
    let dest = root.join("pipeline");
    std::fs::create_dir_all(&dest).map_err(|e| {
        Error::Config(format!("cannot create {}: {e}", dest.display()))
    })?;
    PIPELINE.extract(&dest).map_err(|e| {
        Error::Config(format!("failed to extract pipeline assets to {}: {e}", dest.display()))
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
    share.join("icode").is_file() || Path::new("/opt/mac-k3d/icode").is_file()
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
    }

    #[test]
    fn extract_pipeline_does_not_touch_icode() {
        let root = std::env::temp_dir().join(format!(
            "mac-k3d-extract-icode-{}",
            std::process::id()
        ));
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
}
