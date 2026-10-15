# Eternal Craft Launcher

Fork of [storytold/craft-launcher](https://github.com/storytold/craft-launcher), changed only so
the launcher works with **portable installs** instead of installing apps into `C:\Program Files`.

Nothing else is touched: same UI, same release feed, same SHA-256 verification, same single-instance
lock, same self-update.

Upstream: https://github.com/storytold/craft-launcher
Branch: `main` (one branch, portable only)

## What is different from upstream

| # | Change | File |
| --- | --- | --- |
| 1 | Windows offers the portable zip first, not MSI only | `crates/native/src/install.rs` `available_kinds()` |
| 2 | New `windows::install_zip()` — unpacks a verified release zip into the launcher's apps folder and records the exe | `crates/native/src/install.rs` |
| 3 | `install()` gains a Windows arm for `PackageKind::PortableZip` that returns a record | `crates/native/src/install.rs` |
| 4 | `uninstall()` gains a Windows arm for a launcher-owned portable install | `crates/native/src/install.rs` |
| 5 | New `windows::remove_zip()` — takes out only the version folder the launcher unpacked | `crates/native/src/install.rs` |
| 6 | `apps()` uses the folder picked in Settings, else an `APPS` folder beside the launcher | `crates/native/src/paths.rs` |
| 7 | The picked folder is looked in first, and a portable folder the user put there by hand is adopted and kept | `crates/native/src/detect.rs` `scan()`, `pick()` |
| 8 | Settings shows the install-format picker on Windows too, and an install folder row on every platform | `crates/ui-egui/src/settings.rs`, `crates/ui-egui/src/dialogs.rs` |
| 9 | The launcher's own name, publisher and registry keys are the fork's | `crates/core/src/catalog.rs`, `apps/artcraft-launcher/build.rs`, `crates/native/src/detect.rs`, `crates/native/src/self_update.rs` |
| 10 | Self-update reads this fork's releases, not upstream's | `crates/native/src/self_update.rs` |
| 11 | New dependencies: `zip` 9 (deflate only), `rfd` 0.17 for the folder dialog (desktop only) | `crates/native/Cargo.toml`, `crates/ui-egui/Cargo.toml` |
| 12 | The settings file goes inside the portable folder, so a suite on a stick keeps its settings on another computer | `apps/artcraft-launcher/src/settings_file.rs`, `apps/artcraft-launcher/src/main.rs` |
| 13 | Tests: 4 native + 2 UI + 1 app | `crates/native/src/{install,detect,paths}.rs`, `crates/ui-egui/tests/ui.rs`, `apps/artcraft-launcher/src/settings_file.rs` |
| 13 | Tests: 4 native + 2 UI + 1 app | `crates/native/src/{install,detect,paths}.rs`, `crates/ui-egui/tests/ui.rs`, `apps/artcraft-launcher/src/settings_file.rs` |

Diff against upstream: 110 files, +1371 / -476 (code only: 64 files, +1023 / -359).

### 1. Windows install formats

Upstream:

```rust
#[cfg(windows)]
return vec![PackageKind::Msi];
```

Fork:

```rust
#[cfg(windows)]
return vec![PackageKind::PortableZip, PackageKind::Msi];
```

Upstream could not install a portable zip on Windows at all: the match arm fell through to
"isn't supported on this system", so a portable suite was never recognised and every app showed an
Install button.

### 2. `install_zip`

Unpacks the release zip into the launcher's apps folder, one folder per app, and records the exe.
The record is what makes detection work — `detect()` already trusts `installed.json` before the
registry, so no Windows Installer entries and no `App Paths` keys are needed.

- Entries that would land outside the staging folder are refused (`enclosed_name`).
- The app folder is named the way the suite is kept on disk (uppercase app folder), holding the
  version folder the zip unpacks to, so an app stays in one place and its name always shows the
  version it is.
- An update lands in the same app folder. The old version folder is removed **only when the
  launcher unpacked it**; a folder you put there by hand is never touched.
- Removal takes only the version folder inside the launcher's apps folder, and leaves the app
  folder's own files (its card picture, an icon) alone.

### 3. `apps()`

Upstream unpacks tarball installs into `<data>/apps/<app>`. The fork takes the folder picked in
Settings first, and in portable mode falls back to an `APPS` folder beside the launcher's own folder,
so the whole suite is one portable tree and updates land where the apps already are.

### 4. Detection

Unchanged. `from_manifest()` already reads `installed.json` before anything else; the fork only
makes sure a record can point at a portable exe.

### 5. Choosing the format in the app

Upstream only shows the "Install new apps as" row in Settings on Linux, so a Windows user had no way
to say which format to use — the code order decided it. The fork shows the row on Windows as well,
listing the formats the release actually has.

The choice is stored in `settings.json` as `preferred_kinds` and reaches the installer through
`BackendOptions.kinds` → `install::choose_kinds()`, which puts the preference first. An already
installed app still keeps its own format: the preference only applies to new installs.

Default on Windows is the portable zip, so nothing changes for anyone who never opens Settings.

### 6. Where installs go

Upstream always unpacks into the launcher's own apps folder, so a portable suite had to sit next
the launcher. `paths::apps()` now takes the folder picked in Settings first (`install_dir`, empty by
default), so the launcher can be on one drive and the apps on another.

The row is in Settings on every platform: a text field plus the OS's own folder dialog (`rfd`,
desktop only; the web build shows the field alone). It applies to new installs. A record already in
`installed.json` keeps the path it has, so nothing is moved.

### 7. Adopting a folder the user put there by hand

Windows detection is the installer registry alone, so a portable folder copied onto a drive showed
nothing: no record, no MSI entry. `detect::scan()` reads the apps folder (the uppercase app folder,
a flat lowercase one, and version folders sitting directly under the apps folder), takes the version
from the folder name, and the backend records what it finds. That is what makes an app the launcher
never installed show up and become updatable.

`detect::pick()` decides which copy is reported when the same app exists in more than one place: a
user can have an installer install in Program Files and a portable copy on a drive at the same time,
and the folder Settings points at is the one they asked for, so a copy found there wins. Without a
picked folder the usual order stands, the launcher's own record then the system. The newest copy of
an app wins when a folder holds more than one, because the filesystem lists entries in any order.


## Build from source

Upstream asks for Rust 1.90. In practice `egui` 0.36.2 needs **1.95** — 1.90 fails on this tree,
1.95 builds.

```sh
cargo +1.95 build --release -p artcraft-launcher
cargo +1.95 test -p artcraft-launcher-native
```

## License

Same as upstream: MIT or Apache-2.0. The ArtCraft name, logo and "A" mark are trademarks not
covered by that license and are not used here — forks must remove them.
