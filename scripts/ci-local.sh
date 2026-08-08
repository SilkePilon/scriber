#!/usr/bin/env bash
# Reproduce Scriber's GitHub workflows locally, in the same container image CI
# uses, so a red run can be debugged without waiting on a hosted runner.
#
# This mirrors .github/workflows/ci.yml and .github/workflows/release.yml. When
# you change a workflow, change the matching stage here — the point of the
# script is that it runs the same commands, not a convenient approximation.
set -euo pipefail

IMAGE="fedora:44"          # matches `container:` in both workflows
RUST_TOOLCHAIN="1.97.1"    # matches dtolnay/rust-toolchain in ci.yml
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Named volumes. Everything expensive lives in one of these, so the second run
# of any stage is dramatically faster than the first:
#   dnf      rpm package cache (dnf5 needs keepcache=1 to actually retain them)
#   rustup   toolchain 1.97.1
#   cargo    CARGO_HOME: registry index, .crate downloads, git checkouts
#   target   CARGO_TARGET_DIR, see the note below
#   pyvenv   the venv the cargo-sources check pip-installs into
#   occt     the OCCT source tarball and its configured build tree
#   flatpak  /var/lib/flatpak: org.gnome.Platform//50, Sdk, rust-stable
#   fbcache  flatpak-builder's state dir (downloaded sources, ccache) and its
#            build dir, which have to share a filesystem — see the note on
#            --state-dir in the release stage
VOL_PREFIX="scriber-ci"
VOL_DNF="${VOL_PREFIX}-dnf"
VOL_RUSTUP="${VOL_PREFIX}-rustup"
VOL_CARGO="${VOL_PREFIX}-cargo"
VOL_TARGET="${VOL_PREFIX}-target"
# occt8 gets its own target/: it is a separate job in CI with its own fresh
# checkout, and locally it builds scriber-occt against OCCT 8 headers while the
# test stage builds it against Fedora's 7.9.3. build.rs declares
# rerun-if-env-changed=OCCT_ROOT, so sharing would be *correct* — but each
# stage would invalidate the other's scriber-occt and everything above it on
# every alternation, which is the thrash this script exists to avoid.
VOL_TARGET_OCCT8="${VOL_PREFIX}-target-occt8"
VOL_PYVENV="${VOL_PREFIX}-pyvenv"
VOL_OCCT="${VOL_PREFIX}-occt"
VOL_FLATPAK="${VOL_PREFIX}-flatpak"
VOL_FBCACHE="${VOL_PREFIX}-fbcache"

# WHY THE CONTAINER GETS ITS OWN target/:
# The host builds against Fedora's OCCT at host paths with the host glibc; the
# container builds against the image's. Sharing one target/ makes each build
# invalidate the other's fingerprints, so *both* rebuild from scratch every
# time — the exact opposite of the caching this script exists for. It would
# also drop root-owned files into the working tree. So CARGO_TARGET_DIR points
# at a volume the host never sees, and the host's target/ is left alone.
#
# For the same reason the repository is bind-mounted READ-ONLY at /src and
# copied to /work inside the container. `flatpak-builder` writes build-dir/ and
# .flatpak-builder/ into its working directory, and cargo can rewrite
# Cargo.lock; none of that should land in your checkout as root-owned files.

usage() {
  cat <<'EOF'
scripts/ci-local.sh — run Scriber's CI locally in Docker (fedora:44)

USAGE
  ./scripts/ci-local.sh [OPTIONS] <command>

COMMANDS
  test      the `test` job of .github/workflows/ci.yml:
            cargo fmt --check, clippy -D warnings, cargo test,
            the dynamic-OCCT licensing gate, and the cargo-sources.json
            freshness check
  occt8     the `occt8-shim` job of .github/workflows/ci.yml: configure the
            OCCT version the Flatpak ships and compile the shim against its
            headers
  release   the build half of .github/workflows/release.yml: the full Flatpak
            build (compiles OCCT 8.0.1 from source — tens of minutes on the
            first run), the dynamic-OCCT gate on the shipped binary, and the
            smoke run of that binary
  all       test, then occt8, then release
  clean     delete the cache volumes and start over

OPTIONS
  --sign    release only. Generate a THROWAWAY GPG key inside the container and
            sign the ostree repo and bundle with it, exercising the same signing
            path CI uses. Off by default. Your real signing key is never read.
  -h, --help
            this message

WHAT IT NEVER DOES
  It never reads your real secret key, never touches gh-pages, and never pushes
  anything: the publish and release-upload steps of release.yml are deliberately
  not reproduced. No secrets need to be passed in.

CACHING
  dnf packages, the rustup toolchain, CARGO_HOME, CARGO_TARGET_DIR, the OCCT
  tarball, /var/lib/flatpak and .flatpak-builder all persist in Docker volumes
  named scriber-ci-*, so a second run is dramatically faster than the first.
  The container's target/ is deliberately separate from the host's; see the
  comment at the top of this script. `clean` removes the volumes.

EXAMPLES
  ./scripts/ci-local.sh test
  ./scripts/ci-local.sh occt8
  ./scripts/ci-local.sh --sign release
  ./scripts/ci-local.sh all
EOF
}

die() { echo "ci-local: $*" >&2; exit 2; }

SIGN=0
COMMAND=""
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --sign) SIGN=1 ;;
    test|occt8|release|all|clean)
      [ -z "$COMMAND" ] || die "only one command at a time (got '$COMMAND' and '$1')"
      COMMAND="$1"
      ;;
    *) die "unknown argument '$1'. Try --help." ;;
  esac
  shift
done
[ -n "$COMMAND" ] || { usage; exit 2; }

command -v docker >/dev/null 2>&1 || die "docker is not installed or not on PATH."
docker info >/dev/null 2>&1 || die "cannot talk to the docker daemon."

if [ "$COMMAND" = clean ]; then
  docker volume rm -f "$VOL_DNF" "$VOL_RUSTUP" "$VOL_CARGO" "$VOL_TARGET" \
    "$VOL_TARGET_OCCT8" "$VOL_PYVENV" "$VOL_OCCT" "$VOL_FLATPAK" \
    "$VOL_FBCACHE" >/dev/null 2>&1 || true
  echo "Removed the ${VOL_PREFIX}-* cache volumes."
  exit 0
fi

[ "$SIGN" -eq 0 ] || [ "$COMMAND" = release ] || [ "$COMMAND" = all ] ||
  die "--sign only applies to 'release' (or 'all')."

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

# Copy the checkout into the container's own /work. Excludes mirror the
# manifest's `skip:` list plus target/, which can be gigabytes.
COPY_TREE='
tar -C /src \
  --exclude=./target --exclude=./.git --exclude=./.flatpak-builder \
  --exclude=./build-dir --exclude=./pages --exclude=./repo \
  --exclude=./.superpowers \
  -cf - . | tar -C /work -xf -
cd /work
'

# dnf5 discards downloaded rpms unless keepcache is on, which would make the
# package cache volume useless.
DNF_SETUP='
printf "keepcache=1\n" >> /etc/dnf/dnf.conf
'

RUST_SETUP='
export RUSTUP_HOME=/rustup CARGO_HOME=/cargo
export PATH=/cargo/bin:$PATH
if [ ! -x /cargo/bin/rustup ]; then
  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --no-modify-path --default-toolchain none
fi
rustup toolchain install "'"$RUST_TOOLCHAIN"'" \
  --profile minimal --component rustfmt --component clippy
rustup default "'"$RUST_TOOLCHAIN"'"
export CARGO_TARGET_DIR=/target
'

banner() { printf '\n\033[1m>>> %s\033[0m\n' "$*"; }

# ---------------------------------------------------------------- test job ---
stage_test() {
  cat > "$WORKDIR/step.sh" <<EOF
set -euo pipefail
$DNF_SETUP
# ci.yml: "Install build dependencies" (curl is ours, for rustup)
dnf install -y opencascade-devel cmake gcc-c++ git python3 python3-pip curl
$COPY_TREE
$RUST_SETUP
rustc --version

echo "::: Check formatting"
cargo fmt --check

echo "::: Lint"
cargo clippy --workspace --all-targets -- -D warnings

echo "::: Test"
cargo test --workspace

echo "::: Assert OCCT is dynamically linked"
cargo build --bin scriber
./scripts/check-dynamic-occt.sh "\$CARGO_TARGET_DIR/debug/scriber"

echo "::: Assert the vendored cargo sources are current"
if [ ! -x /pyvenv/bin/python ]; then
  python3 -m venv /pyvenv
  /pyvenv/bin/pip install --quiet aiohttp toml tomlkit
fi
# Same pinned commit as ci.yml: this script is downloaded and executed, so a
# mutable branch ref would let upstream change what runs here.
python3 -c "import urllib.request; urllib.request.urlretrieve('https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/737c0085912f9f7dabf9341d4608e2a77a51a73a/cargo/flatpak-cargo-generator.py', '/tmp/flatpak-cargo-generator.py')"
/pyvenv/bin/python /tmp/flatpak-cargo-generator.py Cargo.lock -o /tmp/cargo-sources.json
if ! diff -u build-aux/cargo-sources.json /tmp/cargo-sources.json; then
  echo "build-aux/cargo-sources.json is stale relative to Cargo.lock." >&2
  echo "Regenerate it with flatpak-cargo-generator.py and commit the result." >&2
  exit 1
fi
EOF
  docker run --rm \
    -v "$REPO_ROOT":/src:ro \
    -v "$WORKDIR":/ci:ro \
    -v "$VOL_DNF":/var/cache/libdnf5 \
    -v "$VOL_RUSTUP":/rustup \
    -v "$VOL_CARGO":/cargo \
    -v "$VOL_TARGET":/target \
    -v "$VOL_PYVENV":/pyvenv \
    -w /work "$IMAGE" bash /ci/step.sh
}

# ---------------------------------------------------------- occt8-shim job ---
stage_occt8() {
  cat > "$WORKDIR/step.sh" <<EOF
set -euo pipefail
$DNF_SETUP
# ci.yml: "Install build dependencies" (curl is ours, for rustup + the tarball)
dnf install -y cmake gcc-c++ git tar gzip curl
$COPY_TREE
$RUST_SETUP

MANIFEST=build-aux/io.github.SilkePilon.Scriber.yaml
# Read the pin out of the manifest instead of repeating it here, exactly as
# the workflow does.
URL=\$(sed -n 's/^ *url: *//p' "\$MANIFEST" | head -1)
SHA=\$(sed -n 's/^ *sha256: *//p' "\$MANIFEST" | head -1)
case "\$URL" in
  *OCCT*.tar.gz) ;;
  *)
    echo "Expected the first manifest source to be the OCCT archive." >&2
    echo "Got: '\$URL'. Update this job alongside the manifest." >&2
    exit 1
    ;;
esac
echo "Checking the shim against \$URL"

# Cached by sha, so only a manifest bump re-downloads.
TARBALL=/occt-cache/\${SHA}.tar.gz
if ! echo "\${SHA}  \$TARBALL" | sha256sum -c - >/dev/null 2>&1; then
  curl -fsSL "\$URL" -o "\$TARBALL"
  echo "\${SHA}  \$TARBALL" | sha256sum -c -
else
  echo "Using the cached tarball \$TARBALL"
fi

SRC=/occt-cache/\${SHA}-src
BUILD=/occt-cache/\${SHA}-build
if [ ! -f "\$SRC/CMakeLists.txt" ]; then
  rm -rf "\$SRC"; mkdir -p "\$SRC"
  tar -xzf "\$TARBALL" -C "\$SRC" --strip-components=1
fi

# CONFIGURE ONLY — compiles no OCCT code. Flags mirror the manifest's
# config-opts so the collected header set is the one that actually ships.
cmake -S "\$SRC" -B "\$BUILD" \\
  -DCMAKE_BUILD_TYPE=Release \\
  -DBUILD_LIBRARY_TYPE=Shared \\
  -DBUILD_MODULE_Draw=OFF \\
  -DBUILD_MODULE_Visualization=OFF \\
  -DBUILD_MODULE_ApplicationFramework=OFF \\
  -DUSE_FREETYPE=OFF \\
  -DUSE_TK=OFF

VERSION_HEADER="\$BUILD/include/opencascade/Standard_Version.hxx"
test -f "\$VERSION_HEADER"
grep '^#define OCC_VERSION_COMPLETE' "\$VERSION_HEADER"

OCCT_ROOT="\$BUILD" cargo check -p scriber-occt
EOF
  docker run --rm \
    -v "$REPO_ROOT":/src:ro \
    -v "$WORKDIR":/ci:ro \
    -v "$VOL_DNF":/var/cache/libdnf5 \
    -v "$VOL_RUSTUP":/rustup \
    -v "$VOL_CARGO":/cargo \
    -v "$VOL_TARGET_OCCT8":/target \
    -v "$VOL_OCCT":/occt-cache \
    -w /work "$IMAGE" bash /ci/step.sh
}

# ------------------------------------------------------------- release job ---
stage_release() {
  cat > "$WORKDIR/step.sh" <<EOF
set -euo pipefail
SIGN=$SIGN
$DNF_SETUP
# release.yml: "Install the flatpak toolchain". No pinentry, matching the
# workflow — see the comment on its "Import the signing key" step.
dnf install -y flatpak flatpak-builder git ostree
$COPY_TREE

echo "::: Add flathub and install the runtime"
flatpak remote-add --if-not-exists flathub \\
  https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install -y --noninteractive flathub \\
  org.gnome.Platform//50 org.gnome.Sdk//50
RUST_BRANCH=\$(flatpak info --show-metadata org.gnome.Sdk//50 |
  sed -n '/^\[Extension org\.freedesktop\.Sdk\.Extension\]\$/,/^\[/{s/^version *= *//p}')
if [ -z "\$RUST_BRANCH" ]; then
  echo "Could not read the Freedesktop SDK extension branch from org.gnome.Sdk//50." >&2
  exit 1
fi
echo "Resolved rust-stable branch to \$RUST_BRANCH"
flatpak install -y --noninteractive flathub \\
  "org.freedesktop.Sdk.Extension.rust-stable//\${RUST_BRANCH}"

GPG_ARGS=()
BUNDLE_ARGS=()
if [ "\$SIGN" = 1 ]; then
  echo "::: Generate a THROWAWAY signing key (your real key is never read)"
  # Same agent configuration release.yml writes before importing the real key.
  # --passphrase '' is load-bearing: without it gpg asks gpg-agent to prompt,
  # and in this image that fails with "No pinentry" before a key even exists.
  mkdir -p /root/.gnupg && chmod 700 /root/.gnupg
  cat > /root/.gnupg/gpg-agent.conf <<'CONF'
allow-loopback-pinentry
allow-preset-passphrase
default-cache-ttl 86400
max-cache-ttl 86400
CONF
  printf "pinentry-mode loopback\nbatch\nno-tty\n" > /root/.gnupg/gpg.conf
  gpgconf --kill gpg-agent
  gpg --batch --passphrase '' --quick-generate-key \\
    "Scriber ci-local throwaway <ci-local@invalid>" default default never
  GPG_KEYID=\$(gpg --list-keys --with-colons ci-local@invalid |
    awk -F: '/^fpr:/{print \$10; exit}')
  echo "Throwaway key: \$GPG_KEYID"
  # Overwrite the *copy* of scriber.gpg so --gpg-keys matches the throwaway
  # key. The host checkout is mounted read-only and is untouched.
  gpg --batch --yes --export "\$GPG_KEYID" > scriber.gpg
  GPG_ARGS=(--gpg-sign="\$GPG_KEYID")
  BUNDLE_ARGS=(--gpg-keys=scriber.gpg --gpg-sign="\$GPG_KEYID")
else
  echo "::: Signing skipped (pass --sign to exercise it with a throwaway key)"
fi

echo "::: Build"
# release.yml's "Fetch the existing repository" step is what creates pages/ in
# CI, by cloning gh-pages. That is deliberately not reproduced here — nothing
# local may read or write the published repo — so the directory is just made
# empty, which is also what the workflow does on a first release.
mkdir -p pages

# --state-dir is ours, not the workflow's: flatpak-builder refuses to run when
# its state dir and its target dir are on different filesystems, and the cache
# volume is by definition a different filesystem from the container's /work.
# Both therefore live on that one volume, which is also what makes
# .flatpak-builder (downloaded sources plus ccache) persist between runs.
# In CI both sit in the checkout on one filesystem, so the flag is unnecessary
# there and the workflow does not carry it.
flatpak-builder --force-clean --disable-rofiles-fuse \\
  --state-dir=/fb/.flatpak-builder \\
  --repo=pages/repo "\${GPG_ARGS[@]}" \\
  /fb/build-dir build-aux/io.github.SilkePilon.Scriber.yaml

echo "::: Assert OCCT is dynamically linked in the shipped binary"
./scripts/check-dynamic-occt.sh /fb/build-dir/files/bin/scriber

echo "::: Run the shipped binary"
# Assertions run inside the sandbox, as in release.yml: --run gives it a
# private /tmp, so the output file does not exist out here.
flatpak-builder --run /fb/build-dir \\
  build-aux/io.github.SilkePilon.Scriber.yaml \\
  sh -euc '
    scriber smoke --output /tmp/release-check.step
    head -c 13 /tmp/release-check.step | grep -q "ISO-10303-21;"
    grep -q "CYLINDRICAL_SURFACE" /tmp/release-check.step
  '

echo "::: Update repository metadata and generate deltas"
flatpak build-update-repo --generate-static-deltas --prune \\
  "\${GPG_ARGS[@]}" pages/repo

echo "::: Build the standalone bundle"
flatpak build-bundle pages/repo \\
  "Scriber-ci-local-x86_64.flatpak" \\
  io.github.SilkePilon.Scriber \\
  --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \\
  "\${BUNDLE_ARGS[@]}"
ls -lh Scriber-ci-local-x86_64.flatpak

# The "Publish to Pages" and "Attach the bundle to the release" steps are
# deliberately NOT reproduced: nothing here may push or touch gh-pages.
echo "::: Publish steps intentionally skipped"
EOF
  docker run --rm --privileged \
    -v "$REPO_ROOT":/src:ro \
    -v "$WORKDIR":/ci:ro \
    -v "$VOL_DNF":/var/cache/libdnf5 \
    -v "$VOL_FLATPAK":/var/lib/flatpak \
    -v "$VOL_FBCACHE":/fb \
    -w /work "$IMAGE" bash /ci/step.sh
}

STAGES=()
case "$COMMAND" in
  test) STAGES=(test) ;;
  occt8) STAGES=(occt8) ;;
  release) STAGES=(release) ;;
  all) STAGES=(test occt8 release) ;;
esac

declare -a RESULTS=()
FAILED=0
for stage in "${STAGES[@]}"; do
  if [ "$FAILED" -ne 0 ]; then
    RESULTS+=("SKIP  $stage")
    continue
  fi
  banner "stage: $stage"
  start=$SECONDS
  if "stage_$stage"; then
    RESULTS+=("PASS  $stage  ($((SECONDS - start))s)")
  else
    RESULTS+=("FAIL  $stage  ($((SECONDS - start))s)")
    FAILED=1
  fi
done

printf '\n===================== ci-local summary =====================\n'
for line in "${RESULTS[@]}"; do printf '  %s\n' "$line"; done
printf '============================================================\n'

if [ "$FAILED" -ne 0 ]; then
  echo "ci-local: FAILED" >&2
  exit 1
fi
echo "ci-local: all requested stages passed."
