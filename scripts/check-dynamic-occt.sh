#!/usr/bin/env bash
# Asserts that the built binary links OCCT dynamically. Static linking would
# exceed the Open CASCADE LGPL exception, so this is a licensing gate, not a
# style preference.
set -euo pipefail

binary="${1:?usage: check-dynamic-occt.sh <path-to-binary>}"

if ! ldd "$binary" | grep -q 'libTKernel'; then
  echo "FAIL: $binary does not dynamically link libTKernel." >&2
  echo "OCCT must be a shared library. Check BUILD_LIBRARY_TYPE and the link flags." >&2
  exit 1
fi

echo "OK: $binary dynamically links OCCT."
ldd "$binary" | grep 'libTK' | sed 's/^/  /'
