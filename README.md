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

Expected fingerprint: 227A 3502 615E D0CA 6211 7A3E 98B3 BA3F 926E 29CD

## License

Dual licensed under MIT or Apache-2.0, at your option.

## Acknowledgements

This software makes use of and is based on facilities provided by the
Open CASCADE Technology software.
