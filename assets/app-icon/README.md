# Eternal Craft Launcher app icon

The app icon is the fork's own mark: the letter **E** in `#7149f9`, drawn by EternalShade3D in
Blender (Mayor Regular), exported as vector paths into
[`docs/brand/eternal-mark.svg`](../../docs/brand/eternal-mark.svg). Every file in this directory is
rendered from it by `packaging/icons.sh`.

It replaces the upstream ArtCraft "A" mark, which forks must remove under
[`docs/brand/LICENSE-brand.txt`](../../docs/brand/LICENSE-brand.txt). The mark here is the fork's
own, licensed like the code (see [`LICENSE.txt`](LICENSE.txt)).

## Files

- `eternal-craft-launcher.svg`: a copy of `docs/brand/eternal-mark.svg`; also the hicolor scalable icon.
- `eternal-craft-launcher-1024.png`: 1024 px render, with extra transparent margin for Apple's icon grid.
- `eternal-craft-launcher.icns`: macOS bundle icon (`CFBundleIconFile`).
- `eternal-craft-launcher.ico`: Windows icon, 16–256 px, embedded in the `.exe` by
  `apps/artcraft-launcher/build.rs` and used by the MSI.
- `hicolor/<size>/apps/ai.storyteller.artcraft-launcher.png` (16–512) and
  `hicolor/scalable/apps/ai.storyteller.artcraft-launcher.svg`: Linux icon theme. The file names are
  the app id, left as upstream's so existing Linux installs keep resolving.
- The tray / menu-bar icon is the same mark, rendered into `docs/brand/eternal-mark-black-44.png`
  (macOS template, 22 pt @2x) and `docs/brand/eternal-mark-64.png` (coloured; Windows, Linux, FreeBSD).

## Regenerate

Run `packaging/icons.sh` on any OS. It needs `resvg`; the `.ico` and `.icns` are packed by
`cargo xtask ico` / `cargo xtask icns`. Never edit these files by hand: change the source SVG in
`docs/brand/` and re-run.

The mark reads as an E from 24 px up. At 16 px the gaps between the arms are one pixel, so it goes
to a blob on a dark taskbar. If that matters, open the counters in the source SVG and re-run.
