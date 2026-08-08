# Implementation language evaluation

**Date:** 2026-08-08
**Question:** Would switching from Rust make Scriber's development faster or better supported? If so, to what?
**Verdict:** No. Stay with Rust.

This was evaluated after Milestone 0 shipped and before Milestone 1 began — the cheapest possible moment to switch.

---

## 1. What the decision actually hinges on

The case for switching rests almost entirely on one cost: the hand-written FFI bridge to OpenCASCADE. Every kernel operation needs a shim function.

That cost is **front-loaded and already paid**. What was expensive was the design, not the per-function work:

- translating `Standard_Failure`, which does not derive from `std::exception` on OCCT 7.x, so an untranslated raise aborts the process
- version-detecting across OCCT 7.9.3 (development) and 8.0.1 (shipped), which differ in `Standard_Failure`'s base class and drop `DynamicType()`
- confining all `unsafe` and all C++ to one crate
- keeping OCCT dynamically linked to stay within the Open CASCADE LGPL exception

Adding an operation to the finished bridge is roughly fifteen lines. Estimating `write_stl` for Milestone 1 bears this out.

So the question is not "is FFI annoying" — it is "does another language save enough elsewhere to justify discarding a working bridge, three crates, and a signed release pipeline".

## 2. Where the remaining work is

Scoring each candidate against the actual roadmap rather than in the abstract.

| Milestone | Dominant work | Rust | C++ | Python | Go |
| --- | --- | --- | --- | --- | --- |
| M1 language | lossless CST, property tests, diagnostics | **best** — `rowan`, `proptest`, `codespan-reporting` | poor | poor | poor |
| M2 viewport | GL, ID-buffer picking, camera | **best** — `gtk4-rs`, `glow` | good | perf ceiling | GC in render loop |
| M3 solver | sparse Levenberg-Marquardt, DOF analysis | **best** | good | slow without numpy | verbose, no operator overloading |
| M4 kernel features | pure OCCT surface | FFI tax lands here | **best** — none | good | worst — two shim layers |
| M5–M8 UI, materials, drawings | GTK4 + libadwaita | **best** | good | viable | weakest bindings |
| M9 git tooling | text processing, CLI | good | poor | good | good |

The language one would switch *to* wins on exactly one milestone — M4 — and loses on the three immediately ahead.

## 3. Ecosystem facts, measured 2026-08-08

| Project | Stars | Last push | Open issues | Note |
| --- | --- | --- | --- | --- |
| `gtk-rs/gtk4-rs` | 2337 | 2026-08-06 | 83 | most active GUI binding of the four |
| `GNOME/gtkmm` | 176 | 2026-08-08 | 0 | official GNOME binding, API/ABI stability guarantee |
| `diamondburned/gotk4` | 687 | 2026-07-30 | 74 | self-documented: "memory leaks and sometimes crashes may occur in certain parts of the API, while other parts might be completely missing" |
| `tpaviot/pythonocc-core` | 1946 | 2026-06-25 | 319 | latest release 7.9.0 (April 2025) |
| `marcuswu/makercad` | 141 | 2026-01-08 | 3 | only Go CAD library; wraps OCCT via a separate `occwrapper` C layer |

## 4. Per-language assessment

### C++ — the strongest alternative, and still not worth it

The only candidate that genuinely eliminates the bridge. OCCT *is* C++, so every OCCT example, forum answer, and header applies directly. `gtkmm` is the official GNOME C++ binding with an API/ABI stability guarantee and zero open issues — institutionally better supported than `gtk4-rs`. It is what FreeCAD does.

Against: it discards memory safety on a codebase whose whole architecture is built around confining unsafety to one crate. It gives up `cargo`, `proptest`, `insta`, and `rowan` — and M1 is precisely a parser-and-property-test milestone, where that toolchain is the point. Build and dependency management regress sharply.

**Verdict:** correct choice if starting from scratch *and* the roadmap were mostly kernel work. Neither holds.

### Python — fastest to features, weakest on this project's actual goals

Biggest CAD ecosystem to borrow from: pythonocc, CadQuery, build123d. Prototyping velocity is unmatched.

Against, and decisively:

1. **pythonocc-core's latest release is 7.9.0 (April 2025); Scriber ships OCCT 8.0.1.** This is the identical staleness objection that led to rejecting a dependency on `opencascade-rs`'s crates.io release and owning the bridge instead. Switching to Python would reintroduce the exact dependency risk that decision removed.
2. It makes the central design goal *harder*. "The document is the program" requires lossless round-trip so the GUI can patch a statement and reprint without disturbing formatting. Python has no `rowan` equivalent. Using Python itself as the DSL was considered and rejected in the Milestone 0 spec, because arbitrary control flow makes GUI round-trip editing unsound.
3. A GUI CAD viewport in Python has a real performance ceiling for tessellation and picking.

**Verdict:** faster only if the goal changes from "this spec" to "any working CAD app".

### Go — the weakest fit, for two structural reasons

1. **cgo cannot call C++ directly.** Go requires C++ → a hand-written `extern "C"` C shim → cgo. That is two layers, both hand-maintained. Rust's `cxx` *generates* the bridge from a declaration. Go's FFI tax is therefore strictly **higher** than the one already paid — the exact opposite of the reason to switch. `makercad` confirms the shape: it needs a separate `occwrapper` C library.
2. **The GTK4 binding is the least mature of the four**, by its own documentation, for an application that is fundamentally a GUI.

Further: no operator overloading makes geometry math verbose, GC pauses sit in the render loop, and there is no lossless-CST library.

**Verdict:** worse on both axes the question asked about — speed and support.

### Rust — keep

Best-supported GTK4 binding by activity. Strongest toolchain for the next three milestones. The bridge is built, works across two OCCT majors, and has a CI job that compiles the OCCT 8 path. `Solid` is deliberately `!Send`/`!Sync` for the coming dedicated-kernel-thread design.

Weakness, stated plainly: no mature Rust CAD ecosystem, so kernel surface is ours to wrap. That is a real, recurring cost concentrated in M4.

## 5. What to do about the real cost instead

The FFI tax is genuine; the answer is not a new language.

1. **Widen the shim in bulk.** When M4 arrives, wrap fillet/chamfer/shell/loft and topology enumeration in one pass rather than one function at a time. The per-function cost is small; the context-switch cost is not.
2. **Consider generating the repetitive parts.** Most shim functions follow one shape: `guard()`, construct an OCCT algorithm, check `IsDone()`, wrap the result. That is mechanical.
3. **Read `opencascade-rs` as a reference.** It is LGPL so its code cannot be copied, but it shows how a given OCCT call is bridged.

## 6. Decision

Stay with Rust. Proceed with Milestone 1 as specified.

Revisit only if a specific trigger fires: `gtk4-rs` becoming unmaintained, or the M4 kernel surface proving several times more expensive than estimated. Neither is in evidence.
