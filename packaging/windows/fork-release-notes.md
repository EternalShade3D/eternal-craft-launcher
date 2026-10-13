Fork build of [ArtCraft Launcher](https://github.com/storytold/craft-launcher): the portable zip
only, no installer, no signing.

What this fork does differently is in [FORK.md](FORK.md) and the README: it installs and updates
Craft apps as portable zips inside the launcher's own `APPS` folder instead of through Windows
Installer into `C:\Program Files`.

Unzip and run `eternal-craft-launcher.exe`. Settings stay in `EternalCraftLauncherData` beside it.

Check the zip against `SHA256SUMS.txt` in this release.

Built by `packaging/windows/package-fork.ps1` with Rust 1.95.
