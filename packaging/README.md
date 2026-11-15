# Packaging and releases

The Makefile installs everything; these only wrap what it produced. That is
on purpose: a packaging script that lists the files again is a second
description of the layout, and two descriptions drift.

| | |
|---|---|
| `deb/control`, `deb/copyright` | metadata for the Debian binary package |
| `rpm/waypointer.spec` | the RPM spec |
| `build-deb.sh`, `build-rpm.sh`, `build-tarball.sh` | build one package into `dist/` |
| `check-version.sh` | refuses a tag that disagrees with `Cargo.toml` |
| `publish-repos.sh` | regenerates the APT and RPM repositories |
| `pages/` | the landing page and the `.repo` file served from GitHub Pages |
| `../.github/release-notes.sh` | the release notes, from `CHANGELOG.md` and the commits |

The AUR packages live in [Felitendo/PKGBUILDS](https://github.com/Felitendo/PKGBUILDS):
`waypointer` builds from source, `waypointer-bin` takes the Arch tarball. Its
CI notices a new GitHub release, updates the checksums and pushes to the AUR.

The packages are per architecture (amd64 and x86_64). The binary links ALSA
and xkbcommon, read off it by `dpkg-shlibdeps` and by rpm. Wayland, fontconfig
and the Vulkan loader are opened at run time, so `deb/control` and the spec
name them by hand, with a Vulkan driver.

The `.deb` is built twice, in Debian 13 and in Ubuntu 26.04, with a suffix on
the version (`~deb13`, `~ubuntu26.04`), and each gets an APT repository of its
own: `deb/trixie` and `deb/resolute`. Debian's own Rust is too old for edition
2024, so the workflow installs Rust with rustup there. The RPM is built with
Fedora's Rust on the oldest Fedora that still gets updates (44), so it runs on
the newer one too; the dry run installs it on both. Move it on when that
Fedora reaches its end of life. The Arch tarball
(`waypointer-<version>-arch-x86_64.tar.zst`) is built on Arch and holds a
folder with the `make install` tree.

## Building one by hand

```bash
packaging/build-deb.sh      # in a Debian 13 container, with the -dev packages from release.yml
packaging/build-rpm.sh      # in a Fedora container, with the -devel packages from the spec
packaging/build-tarball.sh  # in an Arch container, with the packages from release.yml
```

All three take the version from `make version` unless one is passed as the first
argument.

## Making a release

1. Bump the version in `Cargo.toml` (and `Cargo.lock`).
2. Add the release to `CHANGELOG.md`, in the format CLAUDE.md describes.
3. Commit, then `git tag vX.Y.Z && git push origin main vX.Y.Z`.

The `release` workflow builds the packages in Debian, Ubuntu, Fedora and Arch
containers, refuses the tag if it disagrees with `Cargo.toml` or has no
changelog entry, attaches the packages to a GitHub release with the entry as
its notes, and adds the deb and rpm files to the APT and RPM repositories on
the `gh-pages` branch.

## Trying the release path first

```bash
gh workflow run release.yml -f dry_run=true
```

Builds the packages, builds both repositories with a key generated on the
spot, checks the signatures, and installs the packages back out of the
repositories. Nothing is pushed and no release is made.

## Setting up the signing, once

```bash
gpg --batch --passphrase '' --quick-generate-key 'Waypointer repository' rsa4096 sign never

gpg --armor --export-secret-keys 'Waypointer repository' \
    | gh secret set GPG_PRIVATE_KEY
```

Without the secret the workflow still builds the packages and attaches them to
the release; it says so in the log and leaves the repositories alone.

## Pointing Pages at it, once, in this order

1. Set the secret, above.
2. Tag a release. The workflow creates the `gh-pages` branch and fills it.
3. *Then* set **Pages** to deploy from a branch and pick `gh-pages` at the
   root.
