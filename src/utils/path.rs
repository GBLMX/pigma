use std::path::PathBuf;

use dirs::home_dir;

/// Replace illegal characters in a file name with `_`.
pub fn sanitize_filename(s: &str) -> String {
    s.trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// Root directory for the app, adopting the directory it used before the rename.
///
/// The first run after an upgrade moves `pigma` aside instead of starting from an empty config
/// and re-downloading the whole cache. If the move fails — another instance is running, a
/// permission problem — the old directory is used as it is, so the upgrade never loses data.
fn root_dir(base: Option<PathBuf>) -> PathBuf {
    let base = base.unwrap_or_else(|| PathBuf::from("."));
    let current = base.join("boxpigma");
    let legacy = base.join("pigma");

    if !current.exists() && legacy.is_dir() {
        match std::fs::rename(&legacy, &current) {
            Ok(()) => log::info!("adopted {} from before the rename", legacy.display()),
            Err(error) => log::warn!(
                "could not move {} to {} ({error}); staying on the old directory",
                legacy.display(),
                current.display()
            ),
        }
    }

    if current.exists() {
        current
    } else if legacy.exists() {
        legacy
    } else {
        current
    }
}

/// boxpigma cache root directory (cache files and play queues live under it).
pub fn boxpigma_cache_dir() -> PathBuf {
    root_dir(dirs::cache_dir())
}

/// boxpigma config root directory.
pub fn boxpigma_config_dir() -> PathBuf {
    root_dir(dirs::config_dir())
}

pub fn expand_tilde(path: &str) -> PathBuf {
    let home = match home_dir() {
        Some(h) => h,
        None => return PathBuf::from(path),
    };

    if path == "~" {
        return home;
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return home.join(rest);
    }

    if cfg!(windows)
        && let Some(rest) = path.strip_prefix("~\\")
    {
        return home.join(rest);
    }

    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use dirs::home_dir;

    use super::*;

    /// The rename must not cost anyone their config: the first run adopts the old directory.
    #[test]
    fn the_pre_rename_directory_is_adopted_once() {
        let base = std::env::temp_dir().join(format!("boxpigma-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("pigma")).expect("legacy dir");
        std::fs::write(base.join("pigma/config.toml"), "keep me").expect("config file");

        assert_eq!(root_dir(Some(base.clone())), base.join("boxpigma"));
        assert_eq!(
            std::fs::read_to_string(base.join("boxpigma/config.toml")).expect("moved file"),
            "keep me"
        );
        assert!(!base.join("pigma").exists(), "the old directory is gone");

        // Asking again must be a no-op rather than a second move.
        assert_eq!(root_dir(Some(base.clone())), base.join("boxpigma"));

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A fresh install just names the new directory; creating it is the caller's business.
    #[test]
    fn a_fresh_install_uses_the_new_directory() {
        let base = std::env::temp_dir().join(format!("boxpigma-path-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("base dir");

        assert_eq!(root_dir(Some(base.clone())), base.join("boxpigma"));
        assert!(!base.join("boxpigma").exists(), "nothing is created here");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_expand_tilde() {
        let home = home_dir().unwrap();

        assert_eq!(expand_tilde("~"), home);

        let unix_path = expand_tilde("~/.cache/dir/xx");
        assert_eq!(unix_path, home.join(".cache/dir/xx"));

        let win_input = r"~\.cache\dir\xx";
        if cfg!(windows) {
            let expected = home.join(r".cache\dir\xx");
            assert_eq!(expand_tilde(win_input), expected);
        } else {
            assert_eq!(expand_tilde(win_input), PathBuf::from(win_input));
        }
    }
}
