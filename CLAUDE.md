# CLAUDE.md

## Style

- Keep everything short: replies, explanations, comments, docs.
- Use simple English: short sentences, common words.
- Avoid em dashes (—). Do not just swap them for "-" either. Rewrite the sentence instead, for
  example with a comma, a colon, brackets or two sentences.

## Commits

- English only.
- Conventional Commits prefix: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `ci:`, `build:`.
- Subject as short as possible. Imperative, lowercase, no trailing period.
- Body only when something genuinely cannot be inferred from the diff.
- Never add `Co-Authored-By`, "Generated with" or any other AI attribution to commits or PR descriptions.
- Do not commit or push before I have checked the changes locally and said they are fine. Then
  commit and push. Several attempts at the same thing make one commit, and attempts that did not
  work make none. Different things done in one session get a commit each. This also goes for
  releases and tags.

## Layout

Waypointer is the Linux version of [TinyHand](https://github.com/nilbuild/tinyhand) for macOS,
written in Rust and drawn with [Rune](https://github.com/anomalyco/rune) (pinned by rev in
`Cargo.toml`, together with the crates Rune patches; bump them together).

- `src/main.rs`: the command line, one instance at a time, the overlays and windows.
- `src/hub.rs`: what all parts share: settings, on or off, where clicks go. Any thread calls it.
- `src/overlay.rs`: the layer over one screen: hand, laser, ink, ripples, toolbar, hint.
- `src/arm.rs`: where the hand on its stick goes. TinyHand's maths; keep it that way.
- `src/ink.rs`, `src/hands.rs`, `src/art.rs`: lines, the hand pictures, GPU-ready pictures.
- `src/raster.rs`: draws the lines with tiny-skia, in tiles. Rune's own strokes are one
  capsule per segment and look stepped on a hand-drawn line.
- `src/i18n.rs`, `i18n/*.json`: translations. New text goes into every catalog, a test checks.
- `src/handmaker.rs`: the user's own hands from a PNG.
- `src/ui.rs`, `src/icons.rs`: the settings window.
- `src/platform.rs`: D-Bus: the API, KGlobalAccel, KWin scripts, lock and sleep.
- `src/keystate.rs`: Alt, Ctrl, Shift from KWin's keystate protocol (`protocols/keystate.xml`).
- `src/tray.rs`, `src/sound.rs`: the tray icon and the tap sound.
- `assets/`: TinyHand's hands and sounds. `res/`: desktop file, icons, README screenshots.

## Releases

Releases look like the ones of big projects such as Immich: a welcome, highlights, every commit
since the last release sorted by its prefix, and the full changelog link. So commit subjects end
up in public: keep them clear.

1. Read `git log <last tag>..HEAD` and pick the version: only fixes → patch, something new → minor,
   something that breaks or needs the user to act → major.
2. Bump the version in `Cargo.toml` (and `Cargo.lock`) and add the entry at the top of
   `CHANGELOG.md`, in one commit (`chore: 0.2.0`).
3. Push, then push the tag: `git tag v0.2.0 && git push origin main v0.2.0`. The `release`
   workflow makes the GitHub release (notes from `.github/release-notes.sh`), builds the
   packages for Debian, Ubuntu, Fedora and Arch and fills the APT and RPM repositories on
   GitHub Pages. Don't make the release by hand. More in `packaging/README.md`.

A minor or major release (has `### Highlights`, gets a heading):

```markdown
## v0.2.0

_2026-12-01_

Welcome to Waypointer `v0.2.0`! One or two sentences on what this release is about.

### 🚨 Breaking changes

- Only if there are any: what changed, and what the user has to do.

### Highlights

- First highlight, a few words
- Second highlight
```

A patch release is just a sentence or two, for example: "A small patch. The toolbar no longer
hides when you press a button."

- Write for users: what they notice, not how the code does it. Friendly and simple.
- Commands and settings they type go in backticks, buttons and labels in bold.

## README

- New UI (a window, a page, a toolbar button, a setting) gets a screenshot in the README, next
  to the text about it: a real screenshot of it running, in English.
- When a screen changes, take its screenshot again. The README never shows an old one.
- Screenshots show only Waypointer. Crop away everything else on the screen.

## Testing

Never test against the real setup: the settings, the user's own hands and the autostart file
belong to the user.

- `make check` after every change: unit tests, clippy, the desktop file, no dashes.
- `cargo test` covers the arm's maths (the fingertip lands on the cursor), the ink, every
  built-in hand, the hand maker, the settings file and the commands.
- Point a run at files of its own: `WP_CONFIG=<file>` for the settings, `WP_HANDS_DIR=<dir>`
  for the user's hands, `WP_DATA_DIR=<dir>` for hands and sounds.
- Check how the settings window looks without opening it: `waypointer --snap out.png --scene
  pointers` (scenes: general, pointers, pointer-ink, appearance, drawing, sound, about,
  make-hand). The scenes `ink` and `laser` show the drawing and the laser over a light and a
  dark half, `toolbar` the toolbar with its colours open; add `--scale 2` for a HiDPI screen.
  Debug builds have Rune's other headless tools too (`--tree`, `--script`).
- README screenshots of the settings window: `--scene <name>` without `--snap` opens the real
  window in that state, next to the installed Waypointer (no D-Bus, tray or shortcuts). Make it
  the active window through KWin by its pid, then `spectacle -b -n -a -o out.png`.
- A live run on the desktop only with a time limit (`timeout 60 target/debug/waypointer
  --autostart`), and quit the installed Waypointer first: only one runs at a time. The
  overlay takes every click while it is on; Esc or `waypointer off` turns it off.
- `WP_DEBUG=1` logs the displays, KWin's areas and which screen has the pointer.
- Before a live test clicks anything, make sure through KWin that Waypointer's window is the
  active one (`workspace.activeWindow`): the user may be using the desktop.
- The KGlobalAccel component `waypointer` and its shortcuts are the user's. A test that changes
  a shortcut puts it back.
