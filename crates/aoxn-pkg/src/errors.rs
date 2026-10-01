//! Unified package-manager error type and user-facing reporting.



#[derive(Debug, thiserror::Error)]
pub enum PkgError {
    #[error("manifest error: {0}")]
    Manifest(String),

    #[error("lockfile error: {0}")]
    Lockfile(String),

    #[error("registry error: {0}")]
    Registry(String),

    #[error("{0}")]
    Config(String),

    #[error("{message}")]
    Resolve {
        message: String,
        /// extra detail shown with `--explain` (the full PubGrub derivation)
        explain: Option<String>,
    },

    #[error("integrity check failed for {name}@{version}")]
    Integrity { name: String, version: String },

    #[error("offline: {0} (run once online, or drop --offline)")]
    Offline(String),

    #[error("package `{0}` not found in registry")]
    PackageNotFound(String),

    #[error("version conflict: {0}")]
    VersionConflict(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl PkgError {
    pub fn other(msg: impl Into<String>) -> Self {
        PkgError::Other(msg.into())
    }

    /// Extra detail shown when the user passes `--explain`.
    pub fn explain(&self) -> Option<&str> {
        match self {
            PkgError::Resolve { explain: Some(e), .. } => Some(e),
            _ => None,
        }
    }
}

/// Print an error the way the CLI reports it (red `error:` line, plus the
/// `--explain` hint when there is extra detail available).
pub fn report(e: &PkgError) {
    let msg = if format!("{e}").is_empty() {
        String::from("unknown error")
    } else {
        format!("{e}")
    };
    let hint = match e {
        PkgError::Resolve { explain: Some(_), .. } => {
            Some("re-run with --explain for the full conflict derivation")
        }
        PkgError::Integrity { .. } => Some(
            "the downloaded tarball does not match the checksum pinned in aoxn.lock; \
             delete it from the cache (`aoxn cache clean`) and retry, and if it \
             persists the registry copy may have been tampered with",
        ),
        PkgError::PackageNotFound(_) => Some("check the spelling (a typo-squat guard runs on `add`)"),
        _ => None,
    };
    eprintln!("{} {msg}", crate::ui::style_err("error:"));
    if let Some(h) = hint {
        eprintln!();
        eprintln!("{} {h}", crate::ui::style_hint("note:"));
    }
}
