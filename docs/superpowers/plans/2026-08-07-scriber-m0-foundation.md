# Scriber Milestone 0 — Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Rust workspace and prove, end to end, that we can drive OpenCASCADE through our own FFI bridge — producing a box-minus-cylinder solid exported to STEP — then ship that as a signed Flatpak from our own repository.

**Architecture:** A Cargo workspace. All unsafe C++ interop is confined to `scriber-occt`, a `cxx` bridge we write and own, which dynamically links OCCT's shared toolkits. `scriber-kernel` wraps it in a safe, idiomatic Rust API. `scriber-cli` is a thin binary that exercises the stack. No GUI in this milestone.

**Tech Stack:** Rust 2024 edition, `cxx` 1.0 for C++ interop, OpenCASCADE Technology 7.9.3 (development) / 8.0.1 (shipped), `clap` for the CLI, Flatpak + flatpak-builder for distribution, GitHub Actions running in a Fedora 44 container.

## Global Constraints

- Rust edition **2024**; minimum toolchain **1.97.1**.
- OCCT minimum supported version is **7.8** (the release that renamed data-exchange toolkits to `TKDE*`). Development uses Fedora's **7.9.3**; the Flatpak builds **8.0.1**.
- OCCT is **always dynamically linked**. Never statically link OCCT — it would exceed the Open CASCADE LGPL exception.
- All `unsafe` and all C++ lives in `scriber-occt`. No other crate may declare `unsafe` in this milestone.
- Application license is **MIT OR Apache-2.0**. Every crate carries this in its `Cargo.toml`.
- Required attribution, verbatim, in README and About: *"This software makes use of and is based on facilities provided by the Open CASCADE Technology software."*
- Application ID is `io.github.OWNER.Scriber`. `OWNER` is a literal placeholder; the implementer must replace every occurrence with the actual GitHub account name before Task 7.
- Every dependency is pinned to an exact version or commit hash. No floating version ranges beyond Cargo's default caret on published crates.
- Commit after every task. Never commit a failing test suite.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `Cargo.toml` | Workspace root; shared dependency versions and package metadata |
| `rust-toolchain.toml` | Pins the toolchain so CI and local builds agree |
| `LICENSE-MIT`, `LICENSE-APACHE` | Dual license texts |
| `README.md` | Project description, OCCT attribution, install instructions |
| `crates/scriber-occt/Cargo.toml` | Bridge crate manifest |
| `crates/scriber-occt/build.rs` | Compiles the C++ shim, resolves OCCT, emits the link line |
| `crates/scriber-occt/src/lib.rs` | The `#[cxx::bridge]` declaration — the only FFI surface |
| `crates/scriber-occt/src/shim.hpp` | C++ declarations wrapping OCCT types into one opaque `Shape` |
| `crates/scriber-occt/src/shim.cpp` | C++ implementations calling OCCT |
| `crates/scriber-kernel/src/lib.rs` | Safe Rust API — `Solid` and its operations |
| `crates/scriber-kernel/src/error.rs` | Kernel error type |
| `crates/scriber-cli/src/main.rs` | CLI entry point |
| `build-aux/io.github.OWNER.Scriber.yaml` | Flatpak manifest, including the OCCT module |
| `.github/workflows/ci.yml` | Build, test, clippy, and the dynamic-linking assertion |
| `.github/workflows/release.yml` | Tag-triggered Flatpak build, sign, publish to Pages |

`scriber-occt` is deliberately the only crate that knows OCCT exists. If a later milestone needs a new OCCT call, it is added to `shim.hpp`/`shim.cpp`/`lib.rs` and exposed through `scriber-kernel` — never called directly from application code.

---

### Task 1: Workspace skeleton

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `LICENSE-MIT`
- Create: `LICENSE-APACHE`
- Create: `README.md`
- Create: `crates/scriber-kernel/Cargo.toml`
- Create: `crates/scriber-kernel/src/lib.rs`
- Test: `crates/scriber-kernel/src/lib.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Consumes: nothing.
- Produces: a buildable workspace at `crates/*`. Later tasks add members to the `members` array in the root `Cargo.toml`.

- [ ] **Step 1: Create the workspace root manifest**

Create `Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/scriber-kernel"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.97.1"
license = "MIT OR Apache-2.0"
repository = "https://github.com/OWNER/scriber"

[workspace.dependencies]
cxx = "1.0.130"
cxx-build = "1.0.130"
clap = { version = "4.5", features = ["derive"] }
thiserror = "2.0"

[profile.release]
lto = "thin"
codegen-units = 1
```

- [ ] **Step 2: Pin the toolchain**

Create `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.97.1"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Add the license files**

Download the two standard texts verbatim:

```bash
curl -fsSL https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE-APACHE
```

Create `LICENSE-MIT` with the standard MIT text, substituting `2026 Scriber contributors` as the copyright line.

- [ ] **Step 4: Write the README with the required attribution**

Create `README.md`:

```markdown
# Scriber

A local-only 3D CAD application for GNOME. Direct modeling on a B-rep kernel,
where the design document is a readable program.

Scriber does not use the network. There is no account, no sync, and no telemetry.

## Status

Milestone 0 — foundation. Not yet usable for modeling.

## License

Dual licensed under MIT or Apache-2.0, at your option.

## Acknowledgements

This software makes use of and is based on facilities provided by the
Open CASCADE Technology software.
```

- [ ] **Step 5: Create the kernel crate with a failing test**

Create `crates/scriber-kernel/Cargo.toml`:

```toml
[package]
name = "scriber-kernel"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
thiserror.workspace = true
```

Create `crates/scriber-kernel/src/lib.rs`:

```rust
//! Safe geometry API over the OCCT bridge.

/// Returns the crate's semantic version, used to stamp exported files.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        assert_eq!(version(), "0.1.0");
    }
}
```

- [ ] **Step 6: Run the test**

Run: `cargo test -p scriber-kernel`
Expected: PASS, `test tests::version_is_reported ... ok`

- [ ] **Step 7: Verify formatting and lints are clean**

Run: `cargo fmt --check && cargo clippy --workspace -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml rust-toolchain.toml LICENSE-MIT LICENSE-APACHE README.md crates
git commit -m "feat: add cargo workspace skeleton and kernel crate"
```

---

### Task 2: The OCCT bridge — box primitive and volume

This is the highest-risk task in the milestone. It proves the entire FFI approach. Everything after it is incremental.

**Files:**
- Create: `crates/scriber-occt/Cargo.toml`
- Create: `crates/scriber-occt/build.rs`
- Create: `crates/scriber-occt/src/lib.rs`
- Create: `crates/scriber-occt/src/shim.hpp`
- Create: `crates/scriber-occt/src/shim.cpp`
- Modify: `Cargo.toml` (add the new workspace member)
- Test: `crates/scriber-occt/src/lib.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - C++ opaque type `scriber::Shape`, exposed to Rust as `ffi::Shape`.
  - `ffi::make_box(dx: f64, dy: f64, dz: f64) -> UniquePtr<Shape>`
  - `ffi::volume(shape: &Shape) -> f64`

- [ ] **Step 1: Install OCCT development headers**

On Fedora:

```bash
sudo dnf install -y opencascade-devel cmake gcc-c++
```

Verify the headers landed where the build script expects:

```bash
ls /usr/include/opencascade/TopoDS_Shape.hxx
```

Expected: the path prints. If your distribution installs elsewhere, set `OCCT_ROOT` to the prefix containing `include/opencascade` and `lib64`.

- [ ] **Step 2: Create the bridge crate manifest**

Create `crates/scriber-occt/Cargo.toml`:

```toml
[package]
name = "scriber-occt"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
description = "Minimal cxx bridge to OpenCASCADE Technology. Written for Scriber; not derived from any other binding."
links = "occt"

[dependencies]
cxx.workspace = true

[build-dependencies]
cxx-build.workspace = true
```

The `links = "occt"` key declares that this crate owns the OCCT native library, so Cargo will reject a second crate trying to link it.

- [ ] **Step 3: Add the crate to the workspace**

In `Cargo.toml`, change the members array:

```toml
members = ["crates/scriber-kernel", "crates/scriber-occt"]
```

- [ ] **Step 4: Write the C++ header**

Create `crates/scriber-occt/src/shim.hpp`:

```cpp
#pragma once

#include <memory>
#include <stdexcept>
#include <string>
#include <utility>

#include <Standard_Failure.hxx>
#include <Standard_Type.hxx>
#include <TopoDS_Shape.hxx>

#include "rust/cxx.h"

namespace scriber {

// A single opaque wrapper so cxx only has to know about one C++ type.
// TopoDS_Shape is a handle-like value type in OCCT, so copying it is cheap.
struct Shape {
  TopoDS_Shape inner;
};

// OCCT signals failure by raising Standard_Failure, which derives from
// Standard_Transient and NOT from std::exception. cxx's generated catch
// handler only looks for std::exception, so an untranslated OCCT failure
// would unwind straight through an extern "C" frame and abort the process.
//
// Every shim function that calls into OCCT must route through this guard so
// the failure arrives in Rust as an Err instead of terminating.
template <typename Body>
auto guard(Body &&body) -> decltype(body()) {
  try {
    return std::forward<Body>(body)();
  } catch (const Standard_Failure &failure) {
    // Many OCCT failures carry an empty message, so lead with the exception
    // class name (Standard_DomainError, StdFail_NotDone, ...) which is always
    // present and is usually the more diagnostic half.
    const Standard_CString kind = failure.DynamicType()->Name();
    std::string text = kind != nullptr ? kind : "Standard_Failure";

    const Standard_CString message = failure.GetMessageString();
    if (message != nullptr && *message != '\0') {
      text += ": ";
      text += message;
    }

    throw std::runtime_error(text);
  } catch (const std::exception &) {
    // Shim functions throw std::runtime_error themselves for non-raising
    // failures (a boolean that reports IsDone() == false, for example).
    // Rethrow untouched so the specific message survives; cxx converts it.
    throw;
  } catch (...) {
    // OCCT's hierarchy is rooted at Standard_Failure and cxx already handles
    // std::exception, so reaching here should be impossible. Catching anyway
    // costs nothing and keeps a stray throw from aborting the process.
    throw std::runtime_error("unknown C++ exception from OpenCASCADE");
  }
}

std::unique_ptr<Shape> make_box(double dx, double dy, double dz);

double volume(const Shape &shape);

}  // namespace scriber
```

- [ ] **Step 5: Write the C++ implementation**

Create `crates/scriber-occt/src/shim.cpp`:

```cpp
#include "scriber-occt/src/shim.hpp"

#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <GProp_GProps.hxx>

namespace scriber {

std::unique_ptr<Shape> make_box(double dx, double dy, double dz) {
  return guard([&] {
    BRepPrimAPI_MakeBox builder(dx, dy, dz);
    return std::make_unique<Shape>(Shape{builder.Shape()});
  });
}

double volume(const Shape &shape) {
  return guard([&] {
    GProp_GProps props;
    BRepGProp::VolumeProperties(shape.inner, props);
    return props.Mass();
  });
}

}  // namespace scriber
```

`BRepPrimAPI_MakeBox` raises `Standard_DomainError` for a zero or negative
extent, and `.Shape()` raises `StdFail_NotDone` when the build failed. Both now
arrive in Rust as `Err` rather than aborting.

The include path `scriber-occt/src/shim.hpp` is the path cxx generates; it is relative to the crate's parent directory, not the filesystem root.

- [ ] **Step 6: Write the bridge declaration and a failing test**

Create `crates/scriber-occt/src/lib.rs`:

```rust
//! The only place in Scriber where C++ is called.
//!
//! Every function here maps one-to-one onto a small C++ shim in `shim.cpp`
//! that calls OpenCASCADE. Nothing else in the workspace links OCCT.

#[cxx::bridge(namespace = "scriber")]
pub mod ffi {
    unsafe extern "C++" {
        include!("scriber-occt/src/shim.hpp");

        /// An OCCT `TopoDS_Shape`, owned by C++.
        type Shape;

        /// An axis-aligned box with one corner at the origin.
        fn make_box(dx: f64, dy: f64, dz: f64) -> Result<UniquePtr<Shape>>;

        /// Enclosed volume of a solid, in model units cubed.
        fn volume(shape: &Shape) -> Result<f64>;
    }
}

#[cfg(test)]
mod tests {
    use super::ffi;

    #[test]
    fn box_has_expected_volume() {
        let shape = ffi::make_box(2.0, 3.0, 4.0).expect("box builds");
        let volume = ffi::volume(&shape).expect("volume computes");
        assert!(
            (volume - 24.0).abs() < 1e-9,
            "expected volume 24.0, got {volume}"
        );
    }

    #[test]
    fn degenerate_box_is_an_error_not_a_crash() {
        // OCCT raises Standard_DomainError here. If the shim's guard were
        // missing, this would abort the test process instead of returning Err.
        let error = ffi::make_box(0.0, 1.0, 1.0)
            .err()
            .expect("expected a zero-width box to be rejected");

        // Pin the class name too, so a regression to a generic message is caught.
        assert!(
            error.what().contains("Standard_DomainError"),
            "expected the OCCT exception class in the message, got: {}",
            error.what()
        );
    }
}
```

Every function in this bridge returns `Result`. cxx generates the try/catch that
converts the `std::runtime_error` thrown by `guard` into a Rust `Err`.

- [ ] **Step 7: Write the build script**

Create `crates/scriber-occt/build.rs`:

```rust
use std::{env, path::PathBuf};

/// OCCT toolkits required by the current shim. Add to this list only when a new
/// shim function needs symbols from a toolkit that is not already here.
///
/// Verified against the OCCT 8.0.1 source tree:
///   TKernel, TKMath              — FoundationClasses
///   TKG2d, TKG3d, TKGeomBase,
///   TKBRep                       — ModelingData (GProp lives in TKGeomBase)
///   TKGeomAlgo, TKTopAlgo,
///   TKPrim, TKBO, TKShHealing    — ModelingAlgorithms (BRepGProp is in
///                                  TKTopAlgo, BRepPrimAPI in TKPrim,
///                                  BRepAlgoAPI in TKBO)
///   TKXSBase, TKDESTEP           — DataExchange (STEPControl is in TKDESTEP)
const OCCT_TOOLKITS: &[&str] = &[
    "TKernel",
    "TKMath",
    "TKG2d",
    "TKG3d",
    "TKGeomBase",
    "TKBRep",
    "TKGeomAlgo",
    "TKTopAlgo",
    "TKPrim",
    "TKBO",
    "TKShHealing",
    "TKXSBase",
    "TKDESTEP",
];

fn main() {
    let root = PathBuf::from(env::var("OCCT_ROOT").unwrap_or_else(|_| "/usr".into()));
    let include = root.join("include/opencascade");

    assert!(
        include.join("TopoDS_Shape.hxx").exists(),
        "OCCT headers not found at {}. Install opencascade-devel, or set \
         OCCT_ROOT to a prefix containing include/opencascade.",
        include.display()
    );

    cxx_build::bridge("src/lib.rs")
        .file("src/shim.cpp")
        .include(&include)
        .std("c++17")
        .compile("scriber-occt-shim");

    for dir in ["lib64", "lib"] {
        let candidate = root.join(dir);
        if candidate.exists() {
            println!("cargo:rustc-link-search=native={}", candidate.display());
        }
    }

    // dylib, never static: the Open CASCADE LGPL exception covers header
    // material only, so the library itself must remain dynamically linked.
    for toolkit in OCCT_TOOLKITS {
        println!("cargo:rustc-link-lib=dylib={toolkit}");
    }

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/shim.cpp");
    println!("cargo:rerun-if-changed=src/shim.hpp");
    println!("cargo:rerun-if-env-changed=OCCT_ROOT");
}
```

- [ ] **Step 8: Run the test**

Run: `cargo test -p scriber-occt`
Expected: PASS, 2 tests — `box_has_expected_volume` and `degenerate_box_is_an_error_not_a_crash`

If linking fails with an undefined symbol, find the toolkit that defines it and add it to `OCCT_TOOLKITS`:

```bash
nm -D --defined-only /usr/lib64/libTK*.so | grep -B200 'SYMBOL_NAME' | grep '^/usr'
```

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml crates/scriber-occt
git commit -m "feat(occt): add cxx bridge with box primitive and volume"
```

---

### Task 3: Cylinder primitive and boolean cut

**Files:**
- Modify: `crates/scriber-occt/src/shim.hpp`
- Modify: `crates/scriber-occt/src/shim.cpp`
- Modify: `crates/scriber-occt/src/lib.rs`

**Interfaces:**
- Consumes: `ffi::Shape`, `ffi::volume` from Task 2.
- Produces:
  - `ffi::make_cylinder(radius: f64, height: f64) -> UniquePtr<Shape>` — axis along +Z, base at origin.
  - `ffi::cut(target: &Shape, tool: &Shape) -> UniquePtr<Shape>` — `target` minus `tool`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/scriber-occt/src/lib.rs`:

```rust
    #[test]
    fn cut_removes_the_tool_volume() {
        // A 10×10×10 block with a full-height radius-2 cylinder bored out.
        let block = ffi::make_box(10.0, 10.0, 10.0).expect("block builds");
        let drill = ffi::make_cylinder(2.0, 10.0).expect("cylinder builds");
        let bored = ffi::cut(&block, &drill).expect("cut succeeds");

        // The cylinder is centred on the origin corner, so exactly one
        // quarter of it lies inside the block.
        let quarter_cylinder = std::f64::consts::PI * 2.0 * 2.0 * 10.0 / 4.0;
        let expected = 1000.0 - quarter_cylinder;
        let actual = ffi::volume(&bored).expect("volume computes");

        assert!(
            (actual - expected).abs() < 1e-6,
            "expected volume {expected}, got {actual}"
        );
    }
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test -p scriber-occt cut_removes_the_tool_volume`
Expected: FAIL to compile — `cannot find function 'make_cylinder' in module 'ffi'`

- [ ] **Step 3: Declare the two functions in the header**

In `crates/scriber-occt/src/shim.hpp`, add below `make_box`:

```cpp
std::unique_ptr<Shape> make_cylinder(double radius, double height);

std::unique_ptr<Shape> cut(const Shape &target, const Shape &tool);
```

- [ ] **Step 4: Implement them**

In `crates/scriber-occt/src/shim.cpp`, add these includes at the top of the include block:

```cpp
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
```

And add these functions inside `namespace scriber`:

```cpp
std::unique_ptr<Shape> make_cylinder(double radius, double height) {
  return guard([&] {
    BRepPrimAPI_MakeCylinder builder(radius, height);
    return std::make_unique<Shape>(Shape{builder.Shape()});
  });
}

std::unique_ptr<Shape> cut(const Shape &target, const Shape &tool) {
  return guard([&] {
    // The constructor already runs the operation. Calling Build() again would
    // clear and re-run the whole DS filler, doubling the cost of every cut.
    BRepAlgoAPI_Cut op(target.inner, tool.inner);

    if (!op.IsDone()) {
      throw std::runtime_error("boolean cut failed");
    }

    // IsDone() only means the algorithm ran; subtracting a larger solid
    // succeeds and yields an empty compound. Detecting that is the kernel
    // layer's job (Error::EmptyResult), not the bridge's.
    return std::make_unique<Shape>(Shape{op.Shape()});
  });
}
```

Unlike the primitive builders, `BRepAlgoAPI_Cut` reports failure through
`IsDone()` rather than always raising, so it is checked explicitly. Both paths
end up as an `Err` in Rust.

- [ ] **Step 5: Declare them in the bridge**

In `crates/scriber-occt/src/lib.rs`, add inside `unsafe extern "C++"`:

```rust
        /// A cylinder whose axis runs along +Z with its base at the origin.
        fn make_cylinder(radius: f64, height: f64) -> Result<UniquePtr<Shape>>;

        /// Boolean subtraction: `target` with `tool` removed.
        fn cut(target: &Shape, tool: &Shape) -> Result<UniquePtr<Shape>>;
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p scriber-occt`
Expected: PASS, 3 tests green

- [ ] **Step 7: Commit**

```bash
git add crates/scriber-occt
git commit -m "feat(occt): add cylinder primitive and boolean cut"
```

---

### Task 4: STEP export

**Files:**
- Modify: `crates/scriber-occt/src/shim.hpp`
- Modify: `crates/scriber-occt/src/shim.cpp`
- Modify: `crates/scriber-occt/src/lib.rs`

**Interfaces:**
- Consumes: `ffi::Shape`, `ffi::make_box` from Task 2.
- Produces: `ffi::write_step(shape: &Shape, path: &str) -> Result<()>` — `Err` on any OCCT failure, carrying OCCT's own message where it has one.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/scriber-occt/src/lib.rs`:

```rust
    #[test]
    fn write_step_produces_a_valid_header() {
        let shape = ffi::make_box(1.0, 1.0, 1.0).expect("box builds");
        let path = std::env::temp_dir().join("scriber_occt_write_step.step");
        let path_str = path.to_str().expect("temp path is valid UTF-8");

        ffi::write_step(&shape, path_str).expect("write_step succeeds");

        let contents = std::fs::read_to_string(&path).expect("STEP file is readable");
        assert!(
            contents.starts_with("ISO-10303-21;"),
            "file does not start with a STEP header: {:?}",
            &contents[..contents.len().min(40)]
        );
        assert!(contents.contains("END-ISO-10303-21;"), "STEP file is truncated");

        std::fs::remove_file(&path).ok();
    }
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test -p scriber-occt write_step_produces_a_valid_header`
Expected: FAIL to compile — `cannot find function 'write_step' in module 'ffi'`

- [ ] **Step 3: Declare it in the header**

In `crates/scriber-occt/src/shim.hpp`, add:

```cpp
void write_step(const Shape &shape, rust::Str path);
```

- [ ] **Step 4: Implement it**

In `crates/scriber-occt/src/shim.cpp`, add these includes:

```cpp
#include <IFSelect_ReturnStatus.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_PrinterOStream.hxx>
#include <STEPControl_StepModelType.hxx>
#include <STEPControl_Writer.hxx>

#include <string>
```

And this function inside `namespace scriber`:

```cpp
namespace {

// Symbolic name for the status, so the message does not depend on an integer
// whose meaning a reader would have to go look up.
const char *status_name(IFSelect_ReturnStatus status) {
  switch (status) {
    case IFSelect_RetVoid:
      return "IFSelect_RetVoid";
    case IFSelect_RetDone:
      return "IFSelect_RetDone";
    case IFSelect_RetError:
      return "IFSelect_RetError";
    case IFSelect_RetFail:
      return "IFSelect_RetFail";
    case IFSelect_RetStop:
      return "IFSelect_RetStop";
  }

  return "IFSelect_Ret<unknown>";
}

}  // namespace

void write_step(const Shape &shape, rust::Str path) {
  guard([&] {
    STEPControl_Writer writer;

    // OCCT reports the useful detail to its own messenger, never through the
    // exception, so the status is all we can hand back. It becomes the
    // `reason` in Task 5's Error::StepWriteFailed. The path is deliberately
    // NOT included — Rust already knows it and would print it twice.
    const IFSelect_ReturnStatus transferred =
        writer.Transfer(shape.inner, STEPControl_AsIs);
    if (transferred != IFSelect_RetDone) {
      throw std::runtime_error(std::string("STEP transfer failed (") +
                               status_name(transferred) + ")");
    }

    // rust::Str is not null-terminated, so copy before handing to OCCT.
    const std::string target(path.data(), path.size());

    const IFSelect_ReturnStatus written = writer.Write(target.c_str());
    if (written != IFSelect_RetDone) {
      throw std::runtime_error(std::string("STEP write failed (") +
                               status_name(written) + ")");
    }
  });
}
```

**Silencing OCCT's console output.** OCCT's default messenger prints to stdout —
a transfer banner on every write, and diagnostics from other subsystems. A CLI
owns its stdout, so the printers are removed once, eagerly, from the top of
*every* shim entry point rather than lazily inside `write_step`. Doing it lazily
would let kernel chatter escape during the `make_box`/`cut` calls that precede
the first write. Add to `shim.hpp`, inside `namespace scriber`, above `guard()`:

```cpp
// Drops OCCT's console printers the first time any shim function runs, so
// kernel diagnostics never land on stdout. The function-local static makes
// this thread-safe and once-only under C++11 and later.
inline void silence_kernel_console() {
  static const bool done = [] {
    Message::DefaultMessenger()->RemovePrinters(
        STANDARD_TYPE(Message_PrinterOStream));
    return true;
  }();
  (void)done;
}
```

Call `silence_kernel_console();` as the first statement of `make_box`,
`make_cylinder`, `cut`, `volume`, and `write_step`.

`guard()` deduces a `void` return here, which is well-formed — do not add a
dummy return value.

- [ ] **Step 5: Declare it in the bridge**

In `crates/scriber-occt/src/lib.rs`, add inside `unsafe extern "C++"`:

```rust
        /// Writes `shape` to `path` as AP214 STEP.
        fn write_step(shape: &Shape, path: &str) -> Result<()>;
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p scriber-occt`
Expected: PASS, all 4 tests green

- [ ] **Step 7: Commit**

```bash
git add crates/scriber-occt
git commit -m "feat(occt): add STEP export"
```

---

### Task 5: The safe kernel API

**Files:**
- Create: `crates/scriber-kernel/src/error.rs`
- Modify: `crates/scriber-kernel/src/lib.rs`
- Modify: `crates/scriber-kernel/Cargo.toml`

**Interfaces:**
- Consumes: `scriber_occt::ffi::{Shape, make_box, make_cylinder, cut, volume, write_step}`.
- Produces:
  - `scriber_kernel::Error` — enum with variants `InvalidDimension { name: &'static str, value: f64 }`, `Kernel(String)`, `StepWriteFailed { path: PathBuf, reason: String }`, and `EmptyResult`, plus `From<cxx::Exception>`.
  - `scriber_kernel::Solid` with:
    - `Solid::cuboid(dx: f64, dy: f64, dz: f64) -> Result<Solid, Error>`
    - `Solid::cylinder(radius: f64, height: f64) -> Result<Solid, Error>`
    - `Solid::cut(&self, tool: &Solid) -> Result<Solid, Error>`
    - `Solid::volume(&self) -> Result<f64, Error>`
    - `Solid::write_step(&self, path: impl AsRef<Path>) -> Result<(), Error>`

Every constructor and query returns `Result` because the bridge beneath can fail
— OCCT rejects degenerate inputs, and booleans can fail internally. This is what
makes the "safe API" claim actually true.

`Solid` is deliberately not `Sync`. In later milestones all kernel work happens on one dedicated thread, and this type must not accidentally escape it.

- [ ] **Step 1: Add the dependency**

In `crates/scriber-kernel/Cargo.toml`, add under `[dependencies]`:

```toml
scriber-occt = { path = "../scriber-occt", version = "0.1.0" }
```

- [ ] **Step 2: Write the failing test**

Replace the `tests` module in `crates/scriber-kernel/src/lib.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        assert_eq!(version(), "0.1.0");
    }

    #[test]
    fn cuboid_reports_its_volume() {
        let solid = Solid::cuboid(2.0, 3.0, 4.0).expect("cuboid builds");
        assert!((solid.volume().expect("volume computes") - 24.0).abs() < 1e-9);
    }

    #[test]
    fn cutting_reduces_volume() {
        let block = Solid::cuboid(10.0, 10.0, 10.0).expect("block builds");
        let drill = Solid::cylinder(2.0, 10.0).expect("cylinder builds");
        let bored = block.cut(&drill).expect("cut succeeds");

        let bored_volume = bored.volume().expect("bored volume computes");
        let block_volume = block.volume().expect("block volume computes");
        assert!(bored_volume < block_volume);
    }

    #[test]
    fn degenerate_cuboid_is_rejected() {
        let err = Solid::cuboid(0.0, 1.0, 1.0).expect_err("must be rejected");
        assert!(
            matches!(err, Error::InvalidDimension { name: "dx", .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn negative_and_nan_cylinder_dimensions_are_rejected() {
        // OCCT accepts a zero radius and defers a negative height to a later
        // volume query, so these must be caught here rather than in the kernel.
        for (radius, height, expected) in
            [(-1.0, 10.0, "radius"), (0.0, 10.0, "radius"), (1.0, f64::NAN, "height")]
        {
            let Err(err) = Solid::cylinder(radius, height) else {
                panic!("must be rejected: r={radius} h={height}");
            };
            assert!(
                matches!(err, Error::InvalidDimension { name, .. } if name == expected),
                "got {err:?}"
            );
        }
    }

    #[test]
    fn step_export_writes_a_file() {
        let solid = Solid::cuboid(1.0, 1.0, 1.0).expect("cuboid builds");
        let path = std::env::temp_dir().join("scriber_kernel_export.step");

        solid.write_step(&path).expect("export succeeds");
        assert!(path.metadata().expect("file exists").len() > 0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn step_export_reports_an_unwritable_path() {
        let solid = Solid::cuboid(1.0, 1.0, 1.0).expect("cuboid builds");
        let err = solid
            .write_step("/nonexistent-directory/model.step")
            .expect_err("export must fail");
        assert!(matches!(err, Error::StepWriteFailed { .. }));
    }
}
```

- [ ] **Step 3: Run it to confirm it fails**

Run: `cargo test -p scriber-kernel`
Expected: FAIL to compile — `cannot find type 'Solid' in this scope`

- [ ] **Step 4: Write the error type**

Create `crates/scriber-kernel/src/error.rs`:

```rust
use std::path::PathBuf;

/// Errors produced by geometry operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A dimension was not a finite positive number.
    ///
    /// OCCT does not reliably reject these — `make_cylinder(0.0, 1.0)` returns
    /// a valid-looking shape of zero volume, and a negative height only fails
    /// later during a volume query. We validate at the boundary instead.
    #[error("{name} must be a finite positive number, got {value}")]
    InvalidDimension { name: &'static str, value: f64 },

    /// OCCT raised a failure. Carries the kernel's own message.
    #[error("geometry kernel error: {0}")]
    Kernel(String),

    /// OCCT declined to write the STEP file.
    #[error("failed to write STEP file to {path}: {reason}")]
    StepWriteFailed { path: PathBuf, reason: String },

    /// A boolean operation produced no geometry.
    #[error("operation produced an empty result")]
    EmptyResult,
}

impl From<cxx::Exception> for Error {
    fn from(exception: cxx::Exception) -> Self {
        Error::Kernel(exception.what().to_owned())
    }
}
```

- [ ] **Step 5: Write the implementation**

Replace the top of `crates/scriber-kernel/src/lib.rs` (everything above the `tests` module) with:

```rust
//! Safe geometry API over the OCCT bridge.
//!
//! Nothing outside this crate should touch `scriber_occt` directly.

mod error;

use std::{marker::PhantomData, path::Path, rc::Rc};

use cxx::UniquePtr;
use scriber_occt::ffi;

pub use error::Error;

/// A solid body.
///
/// Not `Send` or `Sync`: all kernel work is confined to one dedicated thread.
pub struct Solid {
    inner: UniquePtr<ffi::Shape>,
    _not_sync: PhantomData<Rc<()>>,
}

// `ffi::Shape` is opaque, so Debug cannot be derived — but Result::expect_err
// in the tests requires the Ok type to be Debug.
impl std::fmt::Debug for Solid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Solid { .. }")
    }
}

impl Solid {
    fn from_raw(inner: UniquePtr<ffi::Shape>) -> Self {
        Self { inner, _not_sync: PhantomData }
    }

    /// An axis-aligned box with one corner at the origin.
    pub fn cuboid(dx: f64, dy: f64, dz: f64) -> Result<Self, Error> {
        check_extent("dx", dx)?;
        check_extent("dy", dy)?;
        check_extent("dz", dz)?;

        Ok(Self::from_raw(ffi::make_box(dx, dy, dz)?))
    }

    /// A cylinder whose axis runs along +Z with its base at the origin.
    pub fn cylinder(radius: f64, height: f64) -> Result<Self, Error> {
        check_extent("radius", radius)?;
        check_extent("height", height)?;

        Ok(Self::from_raw(ffi::make_cylinder(radius, height)?))
    }

    /// This solid with `tool` subtracted from it.
    pub fn cut(&self, tool: &Solid) -> Result<Self, Error> {
        let result = Self::from_raw(ffi::cut(&self.inner, &tool.inner)?);

        if result.volume()? <= 0.0 {
            return Err(Error::EmptyResult);
        }

        Ok(result)
    }

    /// Enclosed volume, in model units cubed.
    pub fn volume(&self) -> Result<f64, Error> {
        Ok(ffi::volume(&self.inner)?)
    }

    /// Writes this solid to `path` as a STEP file.
    pub fn write_step(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let path = path.as_ref();
        let as_str = path.to_str().ok_or_else(|| Error::StepWriteFailed {
            path: path.to_path_buf(),
            reason: "path is not valid UTF-8".to_owned(),
        })?;

        // Keep OCCT's own message — it is the only clue about why a write failed.
        ffi::write_step(&self.inner, as_str).map_err(|exception| Error::StepWriteFailed {
            path: path.to_path_buf(),
            reason: exception.what().to_owned(),
        })
    }
}

/// Rejects dimensions OCCT would accept but should not.
fn check_extent(name: &'static str, value: f64) -> Result<(), Error> {
    if !value.is_finite() || value <= 0.0 {
        return Err(Error::InvalidDimension { name, value });
    }

    Ok(())
}

/// Returns the crate's semantic version, used to stamp exported files.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

Add `cxx.workspace = true` to `[dependencies]` in `crates/scriber-kernel/Cargo.toml`, since `UniquePtr` appears in the public signature.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p scriber-kernel`
Expected: PASS, all 7 tests green

- [ ] **Step 7: Check lints**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 8: Commit**

```bash
git add crates/scriber-kernel Cargo.toml
git commit -m "feat(kernel): add safe Solid API over the OCCT bridge"
```

---

### Task 6: The CLI smoke command

**Files:**
- Create: `crates/scriber-cli/Cargo.toml`
- Create: `crates/scriber-cli/src/main.rs`
- Create: `crates/scriber-cli/tests/smoke.rs`
- Modify: `Cargo.toml` (add the workspace member)

**Interfaces:**
- Consumes: `scriber_kernel::{Solid, Error}`.
- Produces: a binary named `scriber` with one subcommand, `smoke --output <PATH>`, which writes a box-minus-cylinder solid as STEP and prints its volume.

- [ ] **Step 1: Add the crate to the workspace**

In `Cargo.toml`:

```toml
members = ["crates/scriber-kernel", "crates/scriber-occt", "crates/scriber-cli"]
```

- [ ] **Step 2: Create the manifest**

Create `crates/scriber-cli/Cargo.toml`:

```toml
[package]
name = "scriber-cli"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "scriber"
path = "src/main.rs"

[dependencies]
clap.workspace = true
scriber-kernel = { path = "../scriber-kernel", version = "0.1.0" }
```

- [ ] **Step 3: Write the failing integration test**

Create `crates/scriber-cli/tests/smoke.rs`:

```rust
use std::process::Command;

#[test]
fn smoke_command_writes_a_step_file() {
    let output_path = std::env::temp_dir().join("scriber_cli_smoke.step");
    std::fs::remove_file(&output_path).ok();

    let output = Command::new(env!("CARGO_BIN_EXE_scriber"))
        .arg("smoke")
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("binary runs");

    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let contents = std::fs::read_to_string(&output_path).expect("STEP file written");
    assert!(contents.starts_with("ISO-10303-21;"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("volume"), "stdout was: {stdout}");

    std::fs::remove_file(&output_path).ok();
}
```

- [ ] **Step 4: Run it to confirm it fails**

Run: `cargo test -p scriber-cli`
Expected: FAIL — no `src/main.rs` yet, so the crate does not build

- [ ] **Step 5: Write the CLI**

Create `crates/scriber-cli/src/main.rs`:

```rust
//! Headless entry point for Scriber.

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use scriber_kernel::Solid;

#[derive(Parser)]
#[command(name = "scriber", version, about = "Scriber CAD, headless")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a reference solid and export it, proving the kernel works.
    Smoke {
        /// Where to write the STEP file.
        #[arg(short, long, default_value = "smoke.step")]
        output: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Smoke { output } => match smoke(&output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
    }
}

fn smoke(output: &PathBuf) -> Result<(), scriber_kernel::Error> {
    let block = Solid::cuboid(10.0, 10.0, 10.0)?;
    let drill = Solid::cylinder(2.0, 10.0)?;
    let bored = block.cut(&drill)?;

    bored.write_step(output)?;

    println!("wrote {} — volume {:.4}", output.display(), bored.volume()?);

    Ok(())
}
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p scriber-cli`
Expected: PASS, `test smoke_command_writes_a_step_file ... ok`

- [ ] **Step 7: Run it by hand and confirm the output**

Run: `cargo run -p scriber-cli -- smoke --output /tmp/smoke.step`
Expected: `wrote /tmp/smoke.step — volume 968.5841`

That number is `1000 − (π × 2² × 10) / 4`: the full block minus the quarter of the
cylinder that lies inside it. If you get exactly `1000.0000`, the boolean silently did
nothing and the bridge is wired up wrong.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/scriber-cli
git commit -m "feat(cli): add smoke command proving the kernel end to end"
```

---

### Task 7: Flatpak packaging

**Files:**
- Create: `build-aux/io.github.OWNER.Scriber.yaml`
- Create: `build-aux/cargo-sources.json` (generated, committed)
- Create: `scripts/check-dynamic-occt.sh`

**Interfaces:**
- Consumes: the `scriber` binary from Task 6.
- Produces: a Flatpak bundle containing `/app/bin/scriber`, dynamically linked against OCCT 8.0.1.

Replace every `OWNER` below with the real GitHub account name before starting.

- [ ] **Step 1: Install the Flatpak toolchain and runtime**

```bash
sudo dnf install -y flatpak flatpak-builder
flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install -y flathub org.gnome.Platform//49 org.gnome.Sdk//49 org.freedesktop.Sdk.Extension.rust-stable//25.08
```

The GNOME 49 runtime is the current supported branch; GNOME 48 reached end of life on 2026-03-24.

- [ ] **Step 2: Write the manifest**

Create `build-aux/io.github.OWNER.Scriber.yaml`:

```yaml
id: io.github.OWNER.Scriber
runtime: org.gnome.Platform
runtime-version: '49'
sdk: org.gnome.Sdk
sdk-extensions:
  - org.freedesktop.Sdk.Extension.rust-stable
command: scriber

# Local-only by design: no network, no host filesystem beyond user documents.
finish-args:
  - --filesystem=xdg-documents

build-options:
  append-path: /usr/lib/sdk/rust-stable/bin
  env:
    CARGO_HOME: /run/build/scriber/cargo
    OCCT_ROOT: /app

modules:
  # Built as a shared library so the Open CASCADE LGPL exception applies.
  - name: opencascade
    buildsystem: cmake-ninja
    config-opts:
      - -DCMAKE_BUILD_TYPE=Release
      - -DBUILD_LIBRARY_TYPE=Shared
      - -DBUILD_MODULE_Draw=OFF
      - -DBUILD_MODULE_Visualization=OFF
      - -DBUILD_MODULE_ApplicationFramework=OFF
      - -DUSE_FREETYPE=OFF
      - -DUSE_TK=OFF
    sources:
      - type: archive
        url: https://github.com/Open-Cascade-SAS/OCCT/archive/refs/tags/V8_0_1.tar.gz
        sha256: 0d6913eae4bcc09a3653ceced6dda1aec11c35a1513d4c06762c9b002092c68a

  - name: scriber
    buildsystem: simple
    build-options:
      env:
        CARGO_NET_OFFLINE: 'true'
    build-commands:
      - cargo build --release --offline --bin scriber
      - install -Dm755 target/release/scriber /app/bin/scriber
    sources:
      - type: dir
        path: ..
      - cargo-sources.json
```

That checksum was verified against the published `V8_0_1` tarball on 2026-08-07. Confirm it still matches before the first build:

```bash
curl -fsSL https://github.com/Open-Cascade-SAS/OCCT/archive/refs/tags/V8_0_1.tar.gz | sha256sum
```

Expected: `0d6913eae4bcc09a3653ceced6dda1aec11c35a1513d4c06762c9b002092c68a`

Flatpak builds are offline, so a wrong hash fails the build immediately rather than silently fetching something else.

- [ ] **Step 3: Generate the vendored Cargo sources**

Flatpak builds have no network, so every crate must be declared:

```bash
pip install --user aiohttp toml
curl -fsSL https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py -o /tmp/flatpak-cargo-generator.py
python3 /tmp/flatpak-cargo-generator.py Cargo.lock -o build-aux/cargo-sources.json
```

Commit the generated `build-aux/cargo-sources.json`. Regenerate it whenever `Cargo.lock` changes.

- [ ] **Step 4: Write the dynamic-linking check**

This guards the license boundary. Create `scripts/check-dynamic-occt.sh`:

```bash
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
```

Make it executable:

```bash
chmod +x scripts/check-dynamic-occt.sh
```

- [ ] **Step 5: Verify the check catches the local build**

Run: `./scripts/check-dynamic-occt.sh target/debug/scriber`
Expected: `OK: target/debug/scriber dynamically links OCCT.` followed by an indented list of `libTK*` libraries

- [ ] **Step 6: Build the Flatpak**

Run:

```bash
flatpak-builder --force-clean --repo=/tmp/scriber-repo build-dir build-aux/io.github.OWNER.Scriber.yaml
```

Expected: a successful build. OCCT takes a long time to compile the first time; this is normal.

- [ ] **Step 7: Install and run it**

```bash
flatpak-builder --run build-dir build-aux/io.github.OWNER.Scriber.yaml scriber smoke --output /tmp/flatpak-smoke.step
head -c 13 /tmp/flatpak-smoke.step
```

Expected: `ISO-10303-21;`

- [ ] **Step 8: Commit**

```bash
git add build-aux scripts
git commit -m "build: add flatpak manifest and dynamic-linking license gate"
```

---

### Task 8: CI and the self-hosted release pipeline

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/release.yml`
- Modify: `README.md` (install instructions)

**Interfaces:**
- Consumes: everything above.
- Produces: a signed OSTree repository published to GitHub Pages, plus a standalone bundle on each release.

- [ ] **Step 1: Write the CI workflow**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    runs-on: ubuntu-latest
    # Fedora 44 ships OCCT 7.9.3, matching the documented development
    # environment. Ubuntu runners carry an OCCT too old for the TKDE* names.
    container: fedora:44
    steps:
      - name: Install build dependencies
        run: dnf install -y opencascade-devel cmake gcc-c++ git

      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.97.1
          components: rustfmt, clippy

      - name: Check formatting
        run: cargo fmt --check

      - name: Lint
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: Test
        run: cargo test --workspace

      - name: Assert OCCT is dynamically linked
        run: |
          cargo build --bin scriber
          ./scripts/check-dynamic-occt.sh target/debug/scriber
```

- [ ] **Step 2: Push and confirm CI is green**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: build, lint, and test in a fedora container"
git push
```

Expected: the CI run succeeds on GitHub. Do not continue until it is green.

- [ ] **Step 3: Create the signing key**

```bash
gpg --quick-generate-key "Scriber Repository Signing <you@example.com>" rsa4096 sign never
gpg --list-keys --keyid-format LONG
```

Note the key ID. Export both halves:

```bash
gpg --export --armor KEYID > scriber.gpg
gpg --export-secret-keys --armor KEYID | base64 -w0 > /tmp/secret-key.b64
```

Add the contents of `/tmp/secret-key.b64` as the repository secret `FLATPAK_GPG_KEY`, and the key ID as `FLATPAK_GPG_KEYID`. Then delete the temporary file:

```bash
shred -u /tmp/secret-key.b64
```

Commit `scriber.gpg` — it is the public half and is meant to be distributed.

- [ ] **Step 4: Write the release workflow**

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags: ['v*']

permissions:
  contents: write
  pages: write
  id-token: write

jobs:
  build:
    runs-on: ubuntu-latest
    container:
      image: fedora:44
      options: --privileged
    steps:
      - name: Install the flatpak toolchain
        run: dnf install -y flatpak flatpak-builder git ostree

      - uses: actions/checkout@v4

      - name: Add flathub and install the runtime
        run: |
          flatpak remote-add --if-not-exists flathub \
            https://dl.flathub.org/repo/flathub.flatpakrepo
          flatpak install -y --noninteractive flathub \
            org.gnome.Platform//49 org.gnome.Sdk//49 \
            org.freedesktop.Sdk.Extension.rust-stable//25.08

      - name: Import the signing key
        env:
          KEY: ${{ secrets.FLATPAK_GPG_KEY }}
        run: echo "$KEY" | base64 -d | gpg --batch --import

      - name: Fetch the existing repository
        # Keeps history so users receive deltas rather than full redownloads.
        run: |
          git clone --branch gh-pages --depth 1 \
            "https://github.com/${GITHUB_REPOSITORY}.git" pages || mkdir -p pages

      - name: Build
        run: |
          flatpak-builder --force-clean --disable-rofiles-fuse \
            --repo=pages/repo \
            --gpg-sign=${{ secrets.FLATPAK_GPG_KEYID }} \
            build-dir build-aux/io.github.OWNER.Scriber.yaml

      - name: Update repository metadata and generate deltas
        run: |
          flatpak build-update-repo \
            --generate-static-deltas --prune \
            --gpg-sign=${{ secrets.FLATPAK_GPG_KEYID }} \
            pages/repo

      - name: Build the standalone bundle
        run: |
          flatpak build-bundle pages/repo \
            "Scriber-${GITHUB_REF_NAME}-x86_64.flatpak" \
            io.github.OWNER.Scriber \
            --gpg-sign=${{ secrets.FLATPAK_GPG_KEYID }}

      - name: Write the remote definitions
        run: |
          cp scriber.gpg pages/scriber.gpg
          KEY_B64=$(base64 -w0 scriber.gpg)
          BASE="https://OWNER.github.io/scriber"

          cat > pages/scriber.flatpakrepo <<EOF
          [Flatpak Repo]
          Title=Scriber
          Url=${BASE}/repo/
          Homepage=https://github.com/OWNER/scriber
          Comment=Local-only 3D CAD for GNOME
          GPGKey=${KEY_B64}
          EOF

          cat > pages/scriber.flatpakref <<EOF
          [Flatpak Ref]
          Title=Scriber
          Name=io.github.OWNER.Scriber
          Branch=master
          Url=${BASE}/repo/
          RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
          IsRuntime=false
          GPGKey=${KEY_B64}
          EOF

      - name: Publish to Pages
        run: |
          cd pages
          git init -b gh-pages 2>/dev/null || true
          git config user.name "github-actions"
          git config user.email "github-actions@github.com"
          git add -A
          git commit -m "release ${GITHUB_REF_NAME}"
          git push --force \
            "https://x-access-token:${{ secrets.GITHUB_TOKEN }}@github.com/${GITHUB_REPOSITORY}.git" \
            gh-pages

      - name: Attach the bundle to the release
        uses: softprops/action-gh-release@v2
        with:
          files: Scriber-*.flatpak
```

- [ ] **Step 5: Add install instructions to the README**

Append to `README.md`:

```markdown
## Install

Scriber is not on Flathub. It ships from its own signed repository.

Add the remote once, then install and receive updates like any other app:

```sh
flatpak remote-add --if-not-exists scriber \
  https://OWNER.github.io/scriber/scriber.flatpakrepo

flatpak install scriber io.github.OWNER.Scriber
```

Or install in one step:

```sh
flatpak install https://OWNER.github.io/scriber/scriber.flatpakref
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
```

- [ ] **Step 6: Tag a release and verify the whole pipeline**

```bash
git add .github/workflows/release.yml README.md scriber.gpg
git commit -m "ci: publish signed flatpak repo to github pages"
git push
git tag v0.1.0
git push origin v0.1.0
```

Expected: the Release workflow succeeds, and `https://OWNER.github.io/scriber/scriber.flatpakrepo` is reachable.

- [ ] **Step 7: Verify installation as a user would**

On a machine that has never built Scriber:

```bash
flatpak remote-add --if-not-exists scriber https://OWNER.github.io/scriber/scriber.flatpakrepo
flatpak install -y scriber io.github.OWNER.Scriber
flatpak run io.github.OWNER.Scriber smoke --output ~/scriber-smoke.step
head -c 13 ~/scriber-smoke.step
```

Expected: `ISO-10303-21;`

- [ ] **Step 8: Commit any fixes**

```bash
git add -A
git commit -m "fix: corrections found during first release"
```

---

## Milestone 0 Definition of Done

- [ ] `cargo test --workspace` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- [ ] `scriber smoke` writes a valid STEP file containing a bored block.
- [ ] `scripts/check-dynamic-occt.sh` confirms OCCT is dynamically linked.
- [ ] CI is green on `main`.
- [ ] A tagged release publishes a signed repository, and a fresh machine can install and run Scriber from it.
- [ ] README carries the required Open CASCADE attribution.

## What Milestone 0 deliberately does not do

No GUI, no DSL, no document model, no sketcher, no rendering. Those are Milestones 1 and 2. The only purpose here is to prove the riskiest dependency works and that we can ship it.
