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

There is no GUI, no modeling, and no document format yet. The only command is
`smoke`: a diagnostic that proves the geometry pipeline — Rust, through our own
C++ bridge, into OpenCASCADE and back out as STEP — works on your machine. It is
not a feature.

Scriber's sandbox can only write to your Documents folder, so send the output
there:

```sh
flatpak run io.github.SilkePilon.Scriber smoke --output ~/Documents/smoke.step
```

A working install prints one line:

```
wrote /home/<user>/Documents/smoke.step — volume 968.5841
```

The solid it builds is a 10 x 10 x 10 block with a quarter of a radius-2,
height-10 cylinder bored out of one corner, so 968.5841 is the analytic answer:
`1000 - (pi * 2^2 * 10) / 4`. If that number comes back, the kernel is computing
real geometry rather than reporting success it did not earn.

The file it leaves behind is a valid STEP model — 428 entities, 19178 bytes,
opening with `ISO-10303-21;` and closing with `END-ISO-10303-21;` — which you
can load in any CAD application that reads STEP.

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
