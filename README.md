# Scriber

A local-only 3D CAD application for GNOME. Direct modeling on a B-rep kernel,
where the design document is a readable program.

Scriber does not use the network. There is no account, no sync, and no telemetry.

## Status

Milestone 1 — the document language. A document can be built into a STEP or STL
file from the command line. Usable for modeling only in that sense: the GUI is a
later milestone.

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

Scriber models are written as documents. Scriber's sandbox can only reach your
Documents folder, so keep them there. Create `~/Documents/plate.scr`:

```
units mm

param width  = 60
param height = 40
param bore   = width / 12

body plate = cuboid(width, height, 12)
body hole  = cylinder(radius = bore, height = 12)
body part  = cut(plate, hole)

export "part.step" from part
export "part.stl"  from part
```

Then build it:

```sh
flatpak run io.github.SilkePilon.Scriber build ~/Documents/plate.scr
```

Both files appear next to the document. The STEP file is a B-rep model any CAD
application can open; the STL is meshed on export. Other commands:

| Command | Does |
| --- | --- |
| `build <doc>` | evaluate and run the document's exports |
| `check <doc>` | report errors without producing geometry |
| `export <doc> -o <file> [--body <name>]` | export one body to a chosen path |
| `fmt <doc> [--check]` | reprint a document |
| `volume <doc> [--body <name>]` | print a body's volume |

A bare number means whatever the document's `units` line declared, and any
literal may carry its own unit: `mm`, `cm`, `m`, `in`, `ft`, `deg` or `rad`.
Lengths and angles do not mix — `1mm + 45deg` is refused rather than guessed at.

A document writes only inside its own directory. An `export` pointing anywhere
else is refused, naming the path it wanted, and runs only if you add
`--allow-outside` — so opening a document someone sent you cannot be a way to
have it write over your files. The path is never quietly redirected.

Everything Scriber can know about a document — names, dimensions, units, export
paths — is checked before anything is written, so a build that fails on a bad
document leaves no geometry and no files behind, rather than a stale export that
looks freshly written.

Milestone 1 is the language. There is no GUI yet, and no sketches, fillets or
selectors — those arrive in later milestones.

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
