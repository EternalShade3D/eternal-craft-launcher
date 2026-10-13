# Eternal Craft Launcher

Fork of [storytold/craft-launcher](https://github.com/storytold/craft-launcher), changed only so
the launcher works with **portable installs** instead of installing apps into `C:\Program Files`.

Nothing else is touched: same UI, same release feed, same SHA-256 verification, same single-instance
lock, same self-update.

Upstream: https://github.com/storytold/craft-launcher
Branch: `portable-zip-install`

## What is different from upstream

| # | Change | File |
| --- | --- | --- |
| 1 | Windows offers the portable zip first, not MSI only | `crates/native/src/install.rs` `available_kinds()` |
| 2 | New `windows::install_zip()` — unpacks a verified release zip into the launcher's apps folder and records the exe | `crates/native/src/install.rs` |
| 3 | `install()` gains a Windows arm for `PackageKind::PortableZip` that returns a record | `crates/native/src/install.rs` |
| 4 | `uninstall()` gains a Windows arm for a launcher-owned portable install | `crates/native/src/install.rs` |
| 5 | New `windows::remove_zip()` — takes out only the version folder the launcher unpacked | `crates/native/src/install.rs` |
| 6 | `apps()` resolves to an `APPS` folder beside the launcher in portable mode | `crates/native/src/paths.rs` |
| 7 | New dependency: `zip` 9, read-only features | `crates/native/Cargo.toml` |
| 8 | Two tests added | `crates/native/src/install.rs`, `crates/native/src/detect.rs` |

Diff against upstream: 5 files, +209 / -2.

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

Upstream unpacks tarball installs into `<data>/apps/<app>`. In portable mode the fork looks for an
`APPS` folder beside the launcher's own folder first, so the whole suite is one portable tree and
updates land where the apps already are.

### 4. Detection

Unchanged. `from_manifest()` already reads `installed.json` before anything else; the fork only
makes sure a record can point at a portable exe.

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
