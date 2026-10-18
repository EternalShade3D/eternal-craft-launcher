//! Portable Windows packages: bounded extraction and replacement of recorded files only.

use std::path::{Component, Path, PathBuf};

use crate::manifest::Record;
use crate::paths::Paths;
use artcraft_launcher_core::{CraftApp, PackageKind};

const MAX_FILES: usize = 10_000;
const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Custom destinations may have OS-managed links in their host ancestors.
pub fn validate_directory(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return Err("Choose an absolute installation folder without parent-directory components.".into());
    }
    // Resolve the closest existing ancestor: /home and /var can be OS-managed
    // symlinks, and a custom destination need not exist yet.
    for ancestor in path.ancestors() {
        match std::fs::canonicalize(ancestor) {
            Ok(resolved) => {
                if !resolved.is_dir() {
                    return Err("The installation folder contains a non-directory component.".into());
                }
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A dangling link is not a missing directory we can safely create.
                if std::fs::symlink_metadata(ancestor).is_ok() {
                    return Err("The installation folder contains a dangling link.".into());
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("The installation folder has no existing ancestor.".into())
}

/// Links are allowed above the ownership boundary, never inside it (including root).
fn validate_owned_directory(root: &Path, path: &Path) -> Result<(), String> {
    validate_directory(path)?;
    if !path.starts_with(root) {
        return Err("An installation path points outside its folder.".into());
    }
    for ancestor in path.ancestors().take_while(|ancestor| ancestor.starts_with(root)) {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) if linked(&meta) || !meta.is_dir() => {
                return Err("The installation folder contains a link or a non-directory component.".into());
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.to_string()),
            _ => {}
        }
    }
    Ok(())
}

fn resolve_missing(path: &Path) -> Result<PathBuf, String> {
    for ancestor in path.ancestors() {
        match std::fs::canonicalize(ancestor) {
            Ok(resolved) => {
                let suffix = path.strip_prefix(ancestor).map_err(|e| e.to_string())?;
                return Ok(resolved.join(suffix));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if std::fs::symlink_metadata(ancestor).is_ok() {
                    return Err("The installation folder contains a dangling link.".into());
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("The installation folder has no existing ancestor.".into())
}

fn same_windows_path(left: &Path, right: &Path) -> bool {
    fn normalized(path: &Path) -> String {
        let text = path.to_string_lossy();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        text.trim_end_matches(['\\', '/']).to_lowercase()
    }
    normalized(left) == normalized(right)
}

fn linked(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn extract(file: &Path, destination: &Path) -> Result<Vec<PathBuf>, String> {
    let file = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("couldn't read the portable ZIP ({e})"))?;
    if archive.len() > MAX_FILES {
        return Err("The portable ZIP contains too many files.".into());
    }
    let mut files = Vec::new();
    let mut total = 0u64;
    let mut seen = std::collections::BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        let relative = entry.enclosed_name().ok_or("The portable ZIP contains an unsafe path.")?;
        if relative.components().any(|c| !matches!(c, Component::Normal(_)))
            || entry.name().contains(['\\', ':'])
            || entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("The portable ZIP contains an unsafe path or link.".into());
        }
        // Windows treats case variants as the same file; reject duplicate and colliding names.
        if !seen.insert(relative.to_string_lossy().to_lowercase()) {
            return Err("The portable ZIP contains duplicate paths.".into());
        }
        let target = destination.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        total = total.checked_add(entry.size()).filter(|size| *size <= MAX_BYTES).ok_or("The portable ZIP is too large.")?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&target).map_err(|e| e.to_string())?;
        use std::io::Read;
        let declared = entry.size();
        let copied = std::io::copy(&mut (&mut entry).take(declared.saturating_add(1)), &mut output).map_err(|e| e.to_string())?;
        if copied != declared {
            return Err("The portable ZIP has an unexpected unpacked size.".into());
        }
        files.push(relative);
    }
    Ok(files)
}

fn recorded_files<'a>(app: &CraftApp, record: &'a Record) -> Result<(&'a Path, Vec<PathBuf>), String> {
    let root = record.path.parent().ok_or("The portable installation has no parent folder.")?;
    // Fork: the folder the launcher owns is the app id, or the version-named folder a release zip
    // carries inside it (`APPS\GRIDCRAFT\gridcraft-0.5.0-windows-x64-portable`). Both are its own.
    let id = app.id;
    let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if record.kind != PackageKind::PortableZip || (name != id && !name.starts_with(&format!("{id}-"))) {
        return Err("The launcher has no valid ownership record for this portable folder.".into());
    }
    let resolved = resolve_missing(root)?;
    if !same_windows_path(root, &resolved) {
        return Err("The recorded portable folder resolves through a changed alias. Reinstall it to choose a safe location.".into());
    }
    validate_owned_directory(root, root)?;
    let mut files = Vec::new();
    for file in &record.files {
        let relative = file.strip_prefix(root).map_err(|_| "An installation record points outside its folder.")?;
        if relative.as_os_str().is_empty() || relative.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err("The installation record contains an unsafe path.".into());
        }
        validate_owned_directory(root, file.parent().ok_or("A recorded file has no parent folder.")?)?;
        if let Ok(meta) = std::fs::symlink_metadata(file)
            && (linked(&meta) || !meta.is_file())
        {
            return Err("A recorded file was replaced by a link or directory.".into());
        }
        files.push(relative.to_path_buf());
    }
    Ok((root, files))
}

/// Keeps existing portable installations at their location and preserves unrecorded user files,
/// unless `replace` allows writing over them.
#[allow(clippy::too_many_arguments)]
pub fn install(
    app: &CraftApp,
    file: &Path,
    version: &str,
    previous: Option<&Record>,
    paths: &Paths,
    directory: Option<&Path>,
    replace: bool,
    commit: &mut dyn FnMut(&Record) -> Result<(), String>,
) -> Result<Record, String> {
    let (parent, mut old_files, keep_record_folder) = match previous {
        Some(record) => {
            let (root, files) = recorded_files(app, record)?;
            (root.to_path_buf(), files, true)
        }
        None => {
            let parent = directory.map(Path::to_path_buf).unwrap_or_else(|| paths.apps());
            validate_directory(&parent)?;
            (resolve_missing(&parent)?, Vec::new(), false)
        }
    };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let staging = paths.cache.join("staging").join(format!("{}-{}-{stamp}", app.id, std::process::id()));
    validate_owned_directory(&paths.cache, &staging)?;
    std::fs::create_dir_all(staging.parent().ok_or("no staging folder")?).map_err(|e| e.to_string())?;
    std::fs::create_dir(&staging).map_err(|e| e.to_string())?;
    let staging = Staging(staging);
    let unpacked = staging.0.join("unpacked");
    std::fs::create_dir(&unpacked).map_err(|e| e.to_string())?;
    let extracted = extract(file, &unpacked)?;
    // Fork: every release zip carries one top folder named after the version
    // (`gridcraft-0.5.0-windows-x64-portable`). That folder is the app's own folder in the suite, so
    // the relative paths are read from inside it and the destination keeps its name. Upstream reads
    // the executable at the root of the archive, which real release zips never have.
    let top = single_top(&extracted);
    let files: Vec<PathBuf> = extracted
        .iter()
        .map(|relative| top.as_ref().map(|t| relative.strip_prefix(t).unwrap_or(relative)).unwrap_or(relative).to_path_buf())
        .collect();
    // The extracted files are on disk under the top folder, so the copy reads from there while the
    // destination is the app's own folder.
    let source = top.as_ref().map(|t| unpacked.join(t)).unwrap_or_else(|| unpacked.clone());
    let destination = if keep_record_folder { parent } else { fork_folder(app, version, &parent, top.as_deref()) };
    validate_owned_directory(&destination, &destination)?;
    let executable = files
        .iter()
        .find(|relative| {
            relative.parent().is_some_and(|p| p.as_os_str().is_empty())
                && relative
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| app.stems().any(|stem| name.eq_ignore_ascii_case(&format!("{stem}.exe"))))
        })
        .ok_or("The portable ZIP does not contain the expected executable at its root.")?
        .clone();
    // Validate the entire transaction before moving any existing program file.
    // Fork: a folder the launcher adopted (a copy the user placed by hand) has no recorded files, so
    // the executable inside it is the app itself. Writing over it is not writing over a stranger's
    // file, which is what makes a hand-built suite work without a Replace prompt.
    let adopted = previous.is_some() && old_files.is_empty();
    for relative in &files {
        let target = destination.join(relative);
        validate_owned_directory(&destination, target.parent().ok_or("no destination folder")?)?;
        if let Ok(meta) = std::fs::symlink_metadata(&target) {
            let recorded = old_files.iter().any(|old| same_windows_path(old, relative));
            if linked(&meta) || !meta.is_file() || !(recorded || replace || adopted) {
                return Err("The installation would overwrite an unrecorded file.".into());
            }
            if !recorded {
                old_files.push(relative.clone());
            }
        }
    }
    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    let backup = staging.0.join("backup");
    let record = Record {
        version: version.to_string(),
        kind: PackageKind::PortableZip,
        path: destination.join(executable),
        installed_at: crate::now(),
        files: files.iter().map(|relative| destination.join(relative)).collect(),
        needs_root: false,
    };
    let mut moved = Vec::new();
    let mut copied = Vec::new();
    let transaction = (|| {
        for relative in &old_files {
            let old = destination.join(relative);
            if !old.exists() {
                continue;
            }
            let saved = backup.join(relative);
            std::fs::create_dir_all(saved.parent().ok_or("no backup folder")?).map_err(|e| e.to_string())?;
            // Copy the rollback copy: cache and custom installation can be on different drives.
            std::fs::copy(&old, &saved).map_err(|e| e.to_string())?;
            std::fs::remove_file(&old).map_err(|e| e.to_string())?;
            moved.push(relative.clone());
        }
        for relative in &files {
            let target = destination.join(relative);
            std::fs::create_dir_all(target.parent().ok_or("no destination folder")?).map_err(|e| e.to_string())?;
            let mut input = std::fs::File::open(source.join(relative)).map_err(|e| e.to_string())?;
            let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&target).map_err(|e| e.to_string())?;
            copied.push(relative.clone());
            std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
        }
        commit(&record)?;
        Ok::<(), String>(())
    })();
    if let Err(error) = transaction {
        for relative in copied {
            let _ = std::fs::remove_file(destination.join(relative));
        }
        for relative in moved {
            if let Err(restore) = std::fs::copy(backup.join(&relative), destination.join(&relative)) {
                // Preserve backups for manual recovery if the disk won't accept a rollback.
                let backup_path = staging.0.clone();
                std::mem::forget(staging);
                return Err(format!("{error}; could not restore the previous version ({restore}). Recovery files: {}", backup_path.display()));
            }
        }
        return Err(error);
    }
    Ok(record)
}

/// Removes only recorded program files. Personal settings and documents remain.
pub fn remove(app: &CraftApp, record: &Record) -> Result<(), String> {
    let (root, files) = recorded_files(app, record)?;
    for relative in &files {
        let file = root.join(relative);
        if file.exists() {
            std::fs::remove_file(&file).map_err(|e| e.to_string())?;
        }
    }
    for relative in &files {
        let mut directory = root.join(relative).parent().map(Path::to_path_buf);
        while let Some(path) = directory.filter(|p| p.starts_with(root)) {
            if std::fs::remove_dir(&path).is_err() || path == root {
                break;
            }
            directory = path.parent().map(Path::to_path_buf);
        }
    }
    Ok(())
}

/// Fork: the one top folder a release zip carries, if every entry sits inside the same one.
/// `gridcraft-0.5.0-windows-x64-portable/gridcraft.exe` gives `gridcraft-0.5.0-windows-x64-portable`;
/// a flat archive gives nothing.
fn single_top(files: &[PathBuf]) -> Option<PathBuf> {
    let first = files.first()?;
    let first_component = first.components().next()?;
    if !matches!(first_component, Component::Normal(_)) || first.components().count() < 2 {
        return None;
    }
    let top = PathBuf::from(first_component.as_os_str());
    let inside = |relative: &PathBuf| -> bool {
        let Some(c) = relative.components().next() else { return false };
        relative.components().count() > 1 && c.as_os_str() == top.as_os_str()
    };
    files.iter().all(inside).then_some(top)
}

/// Fork layout: one folder per app named with the app id in capitals, and inside it the version-named
/// folder the zip itself carries — `APPS\GRIDCRAFT\gridcraft-0.5.0-windows-x64-portable`. A record
/// already points at its own folder, so only new installs get this shape.
fn fork_folder(app: &CraftApp, version: &str, parent: &Path, top: Option<&Path>) -> PathBuf {
    let id = app.id;
    let name =
        top.and_then(|t| t.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| format!("{id}-{version}-windows-x64-portable"));
    parent.join(app.id.to_uppercase()).join(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn archive(root: &Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let path = root.join(name);
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (name, content) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(content).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn host_symlinks_are_allowed_but_owned_links_are_rejected() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let host = dir.path().join("var-home");
        std::fs::create_dir(&host).unwrap();
        let home = dir.path().join("home");
        symlink(&host, &home).unwrap();
        let root = home.join("photocraft");
        assert!(validate_directory(&root).is_ok());
        assert!(validate_owned_directory(&root, &root).is_ok());

        let app = CraftApp::by_id("photocraft").unwrap();
        let paths = Paths::under(&home);
        let zip = archive(dir.path(), "app.zip", &[("photocraft.exe", b"program"), ("lib/runtime.dll", b"library")]);
        let record = install(app, &zip, "0.5.0", None, &paths, Some(&home), false, &mut |_| Ok(())).unwrap();
        assert_eq!(std::fs::read(&record.path).unwrap(), b"program");

        std::fs::remove_file(root.join("lib/runtime.dll")).unwrap();
        std::fs::remove_dir(root.join("lib")).unwrap();
        symlink(&host, root.join("lib")).unwrap();
        assert!(validate_owned_directory(&root, &root.join("lib/nested")).is_err());
        assert!(remove(app, &record).is_err());
        assert!(install(app, &zip, "0.6.0", Some(&record), &paths, None, false, &mut |_| Ok(())).is_err());
        assert_eq!(std::fs::read(&record.path).unwrap(), b"program");

        let linked_root = home.join("linked-app");
        symlink(&root, &linked_root).unwrap();
        assert!(validate_owned_directory(&linked_root, &linked_root).is_err());
        assert!(validate_owned_directory(&root, &home).is_err());
        let dangling = home.join("dangling");
        symlink(home.join("missing"), &dangling).unwrap();
        assert!(validate_directory(&dangling.join("nested")).is_err());
    }

    #[test]
    fn stale_junction_record_cannot_remove_files_after_retargeting() {
        use std::process::Command;

        fn junction(link: &Path, target: &Path) {
            let status = Command::new("cmd.exe").args(["/C", "mklink", "/J"]).arg(link).arg(target).status().unwrap();
            assert!(status.success());
        }

        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        let link = dir.path().join("Apps junction");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        junction(&link, &first);
        let alias_root = link.join("photocraft");
        std::fs::create_dir(&alias_root).unwrap();
        std::fs::write(alias_root.join("photocraft.exe"), b"old program").unwrap();
        let record = Record {
            version: "0.5.0".into(),
            kind: PackageKind::PortableZip,
            path: alias_root.join("photocraft.exe"),
            installed_at: 0,
            files: vec![alias_root.join("photocraft.exe")],
            needs_root: false,
        };
        std::process::Command::new("cmd.exe").args(["/C", "rmdir"]).arg(&link).status().unwrap();
        junction(&link, &second);
        let foreign_root = link.join("photocraft");
        std::fs::create_dir(&foreign_root).unwrap();
        std::fs::write(foreign_root.join("photocraft.exe"), b"foreign program").unwrap();
        assert!(remove(CraftApp::by_id("photocraft").unwrap(), &record).is_err());
        assert_eq!(std::fs::read(foreign_root.join("photocraft.exe")).unwrap(), b"foreign program");
    }

    #[test]
    fn upgrade_downgrade_and_uninstall_preserve_personal_files_and_location() {
        let dir = tempfile::tempdir().unwrap();
        let app = CraftApp::by_id("photocraft").unwrap();
        let paths = Paths::under(dir.path());
        let parent = dir.path().join("My Tools");
        let first = archive(dir.path(), "first.zip", &[("photocraft.exe", b"first"), ("obsolete.dll", b"old")]);
        let mut record = install(app, &first, "0.5.0", None, &paths, Some(&parent), false, &mut |_| Ok(())).unwrap();
        // Fork: the first install gets its own version folder, and every later one stays in it.
        let home = record.path.parent().unwrap().to_path_buf();
        assert_eq!(home.parent().unwrap().file_name().unwrap().to_string_lossy(), "PHOTOCRAFT");
        assert_eq!(home.file_name().unwrap().to_string_lossy(), "photocraft-0.5.0-windows-x64-portable");
        let personal = home.join("settings.json");
        std::fs::write(&personal, b"personal").unwrap();
        for (version, exe) in [("0.6.0", b"new".as_slice()), ("0.4.0", b"older".as_slice())] {
            let executable = if version == "0.4.0" { "PhotoCraft.exe" } else { "photocraft.exe" };
            let package = archive(dir.path(), "next.zip", &[(executable, exe), ("lib/runtime.dll", b"dll")]);
            record = install(app, &package, version, Some(&record), &paths, Some(&dir.path().join("Ignored")), false, &mut |_| Ok(())).unwrap();
            assert_eq!(std::fs::read(&record.path).unwrap(), exe);
            assert_eq!(record.path.parent().unwrap(), std::fs::canonicalize(personal.parent().unwrap()).unwrap());
            assert!(!home.join("obsolete.dll").exists());
            assert_eq!(std::fs::read(&personal).unwrap(), b"personal");
            let mut manifest = crate::manifest::Manifest::default();
            manifest.apps.insert(app.id.into(), record.clone());
            manifest.save(&paths.manifest()).unwrap();
            let restored = crate::manifest::Manifest::load(&paths.manifest());
            let detected = crate::detect::detect(app, &restored, &crate::detect::Snapshot::take()).unwrap();
            assert_eq!(detected.version, artcraft_launcher_core::version::parse(version));
            assert_eq!(detected.location, record.path.to_string_lossy());
        }
        remove(app, &record).unwrap();
        assert!(!record.path.exists());
        assert_eq!(std::fs::read(personal).unwrap(), b"personal");
    }

    #[test]
    fn a_failed_manifest_commit_restores_the_previous_program() {
        let dir = tempfile::tempdir().unwrap();
        let app = CraftApp::by_id("photocraft").unwrap();
        let paths = Paths::under(dir.path());
        let old = archive(dir.path(), "old.zip", &[("photocraft.exe", b"old"), ("lib.dll", b"library")]);
        let record = install(app, &old, "0.5.0", None, &paths, None, false, &mut |_| Ok(())).unwrap();
        let next = archive(dir.path(), "next.zip", &[("photocraft.exe", b"new"), ("new.dll", b"new")]);
        let result = install(app, &next, "0.6.0", Some(&record), &paths, None, false, &mut |_| Err("manifest disk full".into()));
        assert_eq!(result.unwrap_err(), "manifest disk full");
        assert_eq!(std::fs::read(&record.path).unwrap(), b"old");
        let home = record.path.parent().unwrap().to_path_buf();
        assert_eq!(std::fs::read(home.join("lib.dll")).unwrap(), b"library");
        assert!(!home.join("new.dll").exists());
    }

    #[test]
    fn collision_is_rejected_before_replacing_any_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let app = CraftApp::by_id("photocraft").unwrap();
        let paths = Paths::under(dir.path());
        let old = archive(dir.path(), "old.zip", &[("photocraft.exe", b"old")]);
        let record = install(app, &old, "0.5.0", None, &paths, None, false, &mut |_| Ok(())).unwrap();
        let home = record.path.parent().unwrap().to_path_buf();
        let personal = home.join("settings.json");
        std::fs::write(&personal, b"keep").unwrap();
        let next = archive(dir.path(), "next.zip", &[("photocraft.exe", b"new"), ("settings.json", b"overwrite")]);
        assert!(install(app, &next, "0.6.0", Some(&record), &paths, None, false, &mut |_| Ok(())).is_err());
        assert_eq!(std::fs::read(record.path).unwrap(), b"old");
        assert_eq!(std::fs::read(personal).unwrap(), b"keep");
    }

    #[test]
    fn a_new_install_writes_over_unrecorded_files_only_when_asked_to() {
        let dir = tempfile::tempdir().unwrap();
        let app = CraftApp::by_id("photocraft").unwrap();
        // The record check compares the recorded folder against where it resolves to now, so the
        // base has to be the long form: CI's temp folder comes back as `RUNNER~1`.
        let paths = Paths::under(&dir.path().canonicalize().unwrap());
        // Fork: a new install gets its own version folder, so a copy the user placed by hand somewhere
        // else in the apps folder is left alone.
        let hand = paths.apps().join("PHOTOCRAFT").join("photocraft-0.4.0-windows-x64-portable");
        std::fs::create_dir_all(&hand).unwrap();
        std::fs::write(hand.join("photocraft.exe"), b"theirs").unwrap();
        std::fs::write(hand.join("notes.txt"), b"keep").unwrap();
        let zip = archive(dir.path(), "new.zip", &[("photocraft.exe", b"new")]);
        let record = install(app, &zip, "0.5.0", None, &paths, None, false, &mut |_| Ok(())).unwrap();
        assert_ne!(record.path.parent().unwrap(), hand);
        assert_eq!(std::fs::read(hand.join("photocraft.exe")).unwrap(), b"theirs");
        assert_eq!(std::fs::read(hand.join("notes.txt")).unwrap(), b"keep");

        // An adopted copy is a record with no recorded files: the folder is the launcher's own, so
        // updating it does not need a Replace answer.
        let adopted = Record {
            version: "0.4.0".into(),
            kind: PackageKind::PortableZip,
            path: hand.join("photocraft.exe"),
            installed_at: 0,
            files: Vec::new(),
            needs_root: false,
        };
        install(app, &zip, "0.5.0", Some(&adopted), &paths, None, false, &mut |_| Ok(())).unwrap();
        assert_eq!(std::fs::read(hand.join("photocraft.exe")).unwrap(), b"new");
        assert_eq!(std::fs::read(hand.join("notes.txt")).unwrap(), b"keep");

        // A file in the launcher's own folder that it never recorded still needs the answer.
        let home = record.path.parent().unwrap().to_path_buf();
        std::fs::write(home.join("notes.txt"), b"keep").unwrap();
        let with_notes = archive(dir.path(), "with-notes.zip", &[("photocraft.exe", b"new"), ("notes.txt", b"overwrite")]);
        assert!(install(app, &with_notes, "0.6.0", Some(&record), &paths, None, false, &mut |_| Ok(())).is_err());
        assert_eq!(std::fs::read(home.join("notes.txt")).unwrap(), b"keep");
        install(app, &with_notes, "0.6.0", Some(&record), &paths, None, true, &mut |_| Ok(())).unwrap();
        assert_eq!(std::fs::read(home.join("notes.txt")).unwrap(), b"overwrite");
    }

    #[test]
    fn a_release_zip_with_a_top_folder_lands_in_the_fork_layout() {
        let dir = tempfile::tempdir().unwrap();
        let app = CraftApp::by_id("gridcraft").unwrap();
        let paths = Paths::under(dir.path());
        // The shape every ArtCraft release actually has: one folder named after the version.
        let zip = archive(dir.path(), "grid.zip", &[("gridcraft-0.5.0-windows-x64-portable/gridcraft.exe", b"program")]);
        let record = install(app, &zip, "0.5.0", None, &paths, None, false, &mut |_| Ok(())).unwrap();
        let home = record.path.parent().unwrap();
        assert_eq!(home.file_name().unwrap().to_string_lossy(), "gridcraft-0.5.0-windows-x64-portable");
        assert_eq!(home.parent().unwrap().file_name().unwrap().to_string_lossy(), "GRIDCRAFT");
        assert_eq!(std::fs::read(&record.path).unwrap(), b"program");
    }

    #[test]
    fn rejects_traversal_and_case_collisions() {
        let dir = tempfile::tempdir().unwrap();
        for (name, files) in [
            ("traversal.zip", vec![("../escape.exe", b"bad".as_slice())]),
            ("colon.zip", vec![("data:stream", b"bad".as_slice())]),
            ("duplicate.zip", vec![("PhotoCraft.exe", b"one".as_slice()), ("photocraft.exe", b"two".as_slice())]),
        ] {
            let zip = archive(dir.path(), name, &files);
            let output = dir.path().join(name.replace('.', "-"));
            std::fs::create_dir(&output).unwrap();
            assert!(extract(&zip, &output).is_err(), "{name}");
        }
        assert!(!dir.path().join("escape.exe").exists());
        assert!(validate_directory(Path::new("relative/folder")).is_err());
        assert!(validate_directory(&dir.path().join("../escape")).is_err());
    }
}
