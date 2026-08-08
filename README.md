# Scriber

A local-only 3D CAD application for GNOME. Direct modeling on a B-rep kernel,
where the design document is a readable program.

Scriber does not use the network. There is no account, no sync, and no telemetry.

## Status

Milestone 0 — foundation. Not yet usable for modeling.

## Install

Scriber is not on Flathub. It ships from its own signed repository.

Add the remote once, then install and receive updates like any other app:

```sh
flatpak remote-add --if-not-exists scriber \
  https://SilkePilon.github.io/scriber/scriber.flatpakrepo

flatpak install scriber io.github.SilkePilon.Scriber
```

Or install in one step:

```sh
flatpak install https://SilkePilon.github.io/scriber/scriber.flatpakref
```

If you would rather not add a remote, each release has a standalone bundle.
It installs a fixed version and does **not** update automatically:

```sh
flatpak install --bundle Scriber-<version>-x86_64.flatpak
```

The repository is GPG-signed. Verify the key fingerprint before trusting it:

```sh
gpg --show-keys scriber.gpg
```

Expected fingerprint: 571D 6CC1 9961 D9CF 9EE7 9878 CFF5 3738 0F68 A138

## Usage

There is no GUI yet. A design is a text document that Scriber builds into
geometry. Write one — Scriber's sandbox can only reach your Documents folder, so
keep it there:

```
# ~/Documents/bracket.scr
units mm

param width = 60

body plate = cuboid(width, 40, 12)
body hole = cylinder(radius = 5, height = 12)
body bracket = cut(plate, hole)

export "bracket.step" from bracket
```

Then build it:

```sh
flatpak run io.github.SilkePilon.Scriber build ~/Documents/bracket.scr
```

That writes `~/Documents/bracket.step`, a valid STEP model any CAD application
can open. `.stl` works the same way, and is meshed on export.

The other commands:

| Command                      | What it does                                          |
| ---------------------------- | ----------------------------------------------------- |
| `build <doc>`                | Evaluate the document and run its `export` statements. |
| `check <doc>`                | Report every error without building geometry.          |
| `export <doc> -o <file>`     | Write one body where you say, ignoring the document's own `export` statements. |
| `fmt <doc> [--check]`        | Reprint the document; `--check` fails if it differs.   |
| `volume <doc> [--body NAME]` | Print a body's volume.                                 |

A document writes only inside its own directory. An `export` pointing anywhere
else is refused, naming the path it wanted, and runs only if you add
`--allow-outside` — so opening a document someone sent you cannot be a way to
have it write over your files. The path is never quietly redirected.

Everything Scriber can know about a document — names, dimensions, units, export
paths — is checked before anything is written, so a build that fails on a bad
document leaves no geometry and no files behind, rather than a stale export that
looks freshly written.

## Development

CI runs in a `fedora:44` container. You can reproduce it locally in the same
image with Docker, which is much faster than waiting on a hosted runner because
everything expensive is cached in Docker volumes between runs:

```sh
./scripts/ci-local.sh test      # the CI `test` job: fmt, clippy, tests, gates
./scripts/ci-local.sh occt8     # the CI `occt8-shim` job
./scripts/ci-local.sh release   # the Flatpak build, unsigned and unpublished
./scripts/ci-local.sh --help
```

It never reads your signing key and never pushes anything.

## License

Dual licensed under MIT or Apache-2.0, at your option.

## Acknowledgements

This software makes use of and is based on facilities provided by the
Open CASCADE Technology software.
