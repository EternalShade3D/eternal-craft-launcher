//! Where the launcher keeps its own files.

use std::path::PathBuf;

/// The launcher's folders.
#[derive(Clone, Debug)]
pub struct Paths {
    /// Records of what the launcher installed, and tarball installs.
    pub data: PathBuf,
    /// Saved release answers and in-progress downloads.
    pub cache: PathBuf,
    /// Fork: the folder the user picked in Settings for new installs. Set by `Command::Configure`;
    /// `None` means the folder worked out from where the launcher is.
    pub apps_override: Option<PathBuf>,
}

const DIR: &str = if cfg!(any(target_os = "macos", target_os = "windows")) { "Eternal Craft Launcher" } else { "eternal-craft-launcher" };

/// The folder name this fork used before it was renamed. An install that already has settings
/// under the old name keeps them: the folder is renamed once on the next run.
const OLD_DIR: &str = if cfg!(any(target_os = "macos", target_os = "windows")) { "ArtCraft Launcher" } else { "artcraft-launcher" };

/// The path to use for a folder that may exist under the old name. Renames it once; if the rename
/// fails, keeps using the old folder rather than starting an empty one.
fn moved(new: &std::path::Path, old: &std::path::Path) -> PathBuf {
    if new.exists() {
        return new.to_path_buf();
    }
    if old.is_dir() {
        return match std::fs::rename(old, new) {
            Ok(()) => new.to_path_buf(),
            Err(_) => old.to_path_buf(),
        };
    }
    new.to_path_buf()
}

impl Paths {
    pub fn new() -> Result<Self, String> {
        if let Some(root) = portable_root() {
            return Ok(Self::under(&root));
        }
        let base = dirs::data_local_dir().ok_or("no per-user data folder on this system")?;
        let cache_base = dirs::cache_dir().ok_or("no per-user cache folder on this system")?;
        Ok(Self {
            data: moved(&base.join(DIR), &base.join(OLD_DIR)),
            cache: moved(&cache_base.join(DIR), &cache_base.join(OLD_DIR)),
            apps_override: None,
        })
    }

    /// For tests: everything under one folder.
    pub fn under(root: &std::path::Path) -> Self {
        Self { data: root.join("data"), cache: root.join("cache"), apps_override: None }
    }

    pub fn downloads(&self) -> PathBuf {
        self.cache.join("downloads")
    }

    pub fn release_cache(&self) -> PathBuf {
        self.cache.join("releases.json")
    }

    /// The last answer from ArtCraft's release feed, for an instant first paint and as a fallback.
    pub fn feed_cache(&self) -> PathBuf {
        self.cache.join("feed.json")
    }

    /// How long to hold off calling ArtCraft's API (after failures or HTTP 429), across restarts.
    pub fn feed_throttle(&self) -> PathBuf {
        self.cache.join("feed-throttle.json")
    }

    /// A picture of an app's details page, saved after its first download: under the revision of the
    /// pictures repository it was fetched from, so a new revision never reads an old picture.
    pub fn screenshot(&self, app: &str, key: &str) -> PathBuf {
        self.cache.join("screenshots").join(artcraft_launcher_core::details::SCREENSHOTS_REV).join(app).join(format!("{key}.jpg"))
    }

    pub fn manifest(&self) -> PathBuf {
        self.data.join("installed.json")
    }

    /// The launcher's own log files (`artcraft-launcher.log` and the previous runs').
    pub fn logs(&self) -> PathBuf {
        self.data.join("logs")
    }

    /// Where tarball installs (FreeBSD, Linux without AppImage) are unpacked: `<data>/apps/<app>`.
    ///
    /// Fork: in portable mode the suite has an `APPS` folder holding one folder per app, so the
    /// whole suite is one portable tree and updates land where the apps already are. It is looked
    /// for beside the launcher's own folder first (the suite root), then inside it.
    pub fn apps(&self) -> PathBuf {
        if let Some(dir) = &self.apps_override {
            return dir.clone();
        }
        let Some(root) = portable_root() else {
            return self.data.join("apps");
        };
        let exe_dir = root.parent().unwrap_or(&root);
        let suite = exe_dir.parent().unwrap_or(exe_dir);
        [suite.join("APPS"), exe_dir.join("APPS")]
            .into_iter()
            .find(|p| p.is_dir())
            .unwrap_or_else(|| suite.join("APPS"))
    }

    /// The folder an app unpacked in [`Self::apps`] sees as its home: `<data>/apps/<app>/home`.
    /// A Craft App keeps its settings in the XDG folders (`~/.config`, `~/.local/share`,
    /// `~/.cache`), so pointing `HOME` and those variables here keeps every app's configuration
    /// beside it, inside the folder the launcher owns.
    #[cfg(target_os = "linux")]
    pub fn app_home(&self, id: &str) -> PathBuf {
        self.apps().join(id).join("home")
    }
}

/// Writes `bytes` to `path` via a temporary file and a rename, so a crash never leaves half a file.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or("no parent folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {} ({e})", dir.display()))?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("couldn't write {} ({e})", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("couldn't replace {} ({e})", path.display()))
}

/// Portable mode: a `portable.txt` beside the executable keeps everything in
/// `EternalCraftLauncherData` next to it (the Windows portable zip ships one). A folder left
/// behind by an older build under the old name is renamed once, so an existing install keeps its
/// settings.
pub fn portable_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    if !dir.join("portable.txt").is_file() {
        return None;
    }
    Some(moved(&dir.join("EternalCraftLauncherData"), &dir.join("ArtCraftLauncherData")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_left_under_the_old_name_are_moved_to_the_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("ArtCraftLauncherData");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("installed.json"), b"{}").unwrap();
        let new = dir.path().join("EternalCraftLauncherData");
        assert_eq!(moved(&new, &old), new);
        assert!(new.join("installed.json").is_file());
        // A folder already under the new name wins: nothing is moved into it.
        std::fs::create_dir_all(&old).unwrap();
        assert_eq!(moved(&new, &old), new);
        assert!(old.is_dir());
    }

    #[test]
    fn the_folder_picked_in_settings_wins_over_the_launcher_s_own() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = Paths::under(dir.path());
        // Nothing picked: the folder worked out from where the launcher is, as before.
        assert_eq!(paths.apps(), dir.path().join("data").join("apps"));
        // A folder the user picked, anywhere, including another drive.
        let picked = dir.path().join("MyApps");
        paths.apps_override = Some(picked.clone());
        assert_eq!(paths.apps(), picked);
        // Clearing the row in Settings goes back to the launcher's own folder.
        paths.apps_override = None;
        assert_eq!(paths.apps(), dir.path().join("data").join("apps"));
    }
}
