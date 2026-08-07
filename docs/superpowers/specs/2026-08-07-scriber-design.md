# Scriber — Design

**Date:** 2026-08-07
**Status:** Approved design, pending implementation plan
**Codename in code:** `scriber`

---

## 1. Summary

Scriber is a local-only 3D CAD application for GNOME, written in Rust with GTK4 and
libadwaita. It reproduces Shapr3D's modeling workflow — direct push-pull modeling on a
B-rep kernel, with an adaptive interface that predicts the next tool from the current
selection — and adds three capabilities Shapr3D does not have:

1. The design document **is** a readable program.
2. The project format is open, text-first, and diffable by git.
3. Everything runs headless from a CLI.

Scriber never talks to a network. There is no account, no sync, no telemetry.

### Naming

The app is `Scriber`. Crates are namespaced `scriber-*`. The application ID is
`io.github.<owner>.Scriber` (owner to be fixed before first release). The repository
directory is currently named `Open3D`, which collides with the established Open3D
library (open3d.org); rename it to `Scriber` at the owner's convenience.

Scriber is **not** distributed through Flathub. See Section 12 for the self-hosted
Flatpak repository.

### Trade dress

Cloning workflow, interaction model, and feature set is legitimate. Copying Shapr3D's
icon set, exact chrome, or brand assets is not. All icons are drawn fresh in the
Adwaita style.

---

## 2. Scope

### 2.1 Parity targets

These reproduce Shapr3D's behavior.

| Area | Features |
| --- | --- |
| Sketch | line, arc, circle, rectangle, polygon, ellipse, spline, offset, trim, extend, mirror, pattern; sketch on face; construction planes |
| Constraints | horizontal, vertical, perpendicular, parallel, tangent, coincident, equal, concentric, symmetric, fixed; dimensional constraints; auto-constrain on draw; fully-defined indicator |
| Model | push-pull, extrude, revolve, loft, sweep, shell, fillet, chamfer, draft, split, replace face, offset face, project, linear/circular/mirror patterns, wrap, emboss |
| Boolean | union, subtract, intersect |
| History | feature tree, rollback marker, edit in place, variables and expressions with units, driving sketches |
| Direct | non-history edits on imported or dumb bodies |
| Import/export | STEP, IGES, STL, 3MF, OBJ, DXF and DWG (2D), Parasolid X_T read |
| Visualization | materials, HDRI environment, section views, measure, orthographic and perspective, exploded view |
| Drawings | 2D views from a model, dimensions, annotations, DXF and PDF export |

### 2.2 Beyond Shapr3D

1. **Document-as-program.** A purpose-built CAD DSL is the save format. GUI actions
   write statements; editing statements changes the model. Both directions are lossless.
2. **Open, git-native project format.** A directory of text plus assets. Branch, diff,
   and merge designs with ordinary git.
3. **Headless CLI.** Build, export, render, and diff without a display, so designs can
   be produced and checked in CI.
4. **Local-only guarantee.** No network code paths at all.

### 2.3 Explicitly out of scope

Cloud sync, team spaces, sharing links, AR and visionOS, AI rendering, CAM and
toolpaths, sheet metal, FEA, and assemblies with joints and kinematics. Each of these
is a separate project. The architecture must not preclude them, but this spec does not
cover them.

---

## 3. Decisions and rationale

| Decision | Choice | Why |
| --- | --- | --- |
| Geometry kernel | OpenCASCADE Technology (OCCT) 8.0.1 | The only open kernel with production-grade NURBS, booleans, fillet, shell, loft, sweep, and STEP/IGES. Actively developed — 8.0.1 released 2026-07-30, over 500 changes since 7.9.0. Pure-Rust `truck` would mean writing kernel features for years; Fornjot shut down without reaching its goals. |
| Kernel bindings | Our own `cxx` bridge, written in-house as `scriber-occt` | We do not depend on a third-party binding crate. `opencascade-rs` is alive but its crates.io release is from 2023, and its source is LGPL-2.1 without exception, so vendoring it would force the application to LGPL. Writing our own bridge means we own the FFI, extend it exactly when a milestone needs it, and keep the app permissive. It may be read for reference; its code is not copied. |
| Kernel license | OCCT is LGPL-2.1 **with the Open CASCADE exception**; linked dynamically | The exception permits distributing object code that incorporates OCCT header material (inline functions, templates) under terms of our choice, provided we give prominent notice that the software is based on OCCT. It does **not** waive LGPL relinking for the library itself, so OCCT is linked dynamically. Together this keeps Scriber under MIT/Apache-2.0. |
| Attribution | Required | The OCCT exception obliges prominent notice in supporting documentation that Scriber makes use of and is based on facilities provided by OpenCASCADE Technology. This appears in the README, the About dialog, and `docs/`. |
| Constraint solver | Written in-house, Rust | SolveSpace's `slvs` is GPLv3 and would relicense the whole app. FreeCAD's `planegcs` is LGPL but adds a second C++ FFI surface entangled with FreeCAD's build. Solving this ourselves keeps the codebase one language and integrates directly with the DSL. |
| UI fidelity | GNOME HIG wins; Shapr3D interaction model kept | Adwaita widgets, system theming, GNOME shortcuts. The behavior — adaptive tool prediction, push-pull, minimal chrome — is Shapr3D's. |
| Window layout | Edge-to-edge viewport with floating tool overlays | Chosen by the user. Built with `GtkOverlay` plus the Adwaita `.osd` style class, which is the same pattern Loupe, Totem, and Camera use for controls over a canvas. Native-looking rather than custom chrome. |
| Script surface | Full-window view switcher | `AdwViewSwitcher` toggles Model / Script / Drawing. The script is a first-class view of the document, not a console. |
| Document model | The document is the program | One source of truth makes undo, scripting, and diffing the same mechanism instead of three. |
| Document language | Purpose-built DSL, plus Rhai for automation | The GUI must parse, patch, and print the document losslessly. Arbitrary control flow in the document would make that unsound — the GUI cannot safely patch a `for` loop it did not write. Rhai scripts emit DSL for generative work. |
| Entity references | Persistent ID with geometric-query fallback and a repair UI | Opaque IDs alone would destroy diffability; queries alone silently match the wrong entity after an edit. The hybrid keeps the script readable and survives upstream changes. |
| Viewport | `GtkGLArea` with OpenGL via `glow` | The supported path; GTK4's own renderer is GL. GTK4 has no wgpu widget (gtk4-rs#1278) and the offscreen-texture route is unproven. Renderer sits behind a trait so wgpu can replace it later. |
| Input | Mouse and keyboard first; touch and pen supported | Hover previews, right-click context, numeric entry, scroll zoom. GTK4 gesture controllers drive the same state machines for touch and stylus, but do not shape the layout. |
| Distribution | Flatpak from a self-hosted, signed repository | Flatpak is standard for GNOME and bundles the OpenCascade dependency cleanly. Flathub is not used; the project publishes its own OSTree repository on GitHub Pages and users add it as a remote. See Section 12. |

---

## 4. Architecture

### 4.1 Crates

```
scriber-occt     Our own `cxx` bridge to OCCT's C++ API. Unsafe FFI lives here and
                 nowhere else. Grows one milestone at a time; no third-party
                 binding crate. Dynamically links libTK* from OCCT 8.0.x.
scriber-kernel   Safe, idiomatic Rust geometry API over scriber-occt. Solids,
                 booleans, features, tessellation, STEP / IGES / STL / 3MF / OBJ /
                 X_T. Runs on one dedicated thread.
scriber-solver   2D constraint solver. Sparse Levenberg-Marquardt, DOF analysis,
                 auto-constrain heuristics.
scriber-lang     The DSL. Lexer, parser, AST, lossless printer, units, expressions,
                 evaluator.
scriber-doc      Document model. Feature DAG, incremental rebuild, selector
                 resolution, undo/redo as script transactions.
scriber-render   Scene buffers, GL renderer, ID-buffer picking, gizmos, grid,
                 section planes, view cube.
scriber-ui       GTK4 + libadwaita shell. Tool state machines, sketch editor,
                 history scrubber, script view, repair dialogs.
scriber-cli      Headless entry point: build, export, render, diff.
```

Each crate is independently testable and has a single stated purpose. `scriber-ui`
depends on everything; nothing depends on `scriber-ui`.

### 4.2 Data flow

Geometry is only ever mutated by editing the document. There is no side channel.

```
user gesture
  → tool state machine (scriber-ui)
  → document edit transaction
  → scriber-lang patches the AST
  → scriber-doc marks dependent features dirty
  → kernel thread re-evaluates only the dirty subgraph
  → tessellation deltas
  → scriber-render
  → GtkGLArea frame
```

Because every action is a script edit, undo, scripting, rollback, and git diff are all
the same mechanism observed from different angles.

### 4.3 Threading

- **UI thread** owns GTK, the tool state machines, and the renderer. It never blocks.
- **Kernel thread** exclusively owns all OpenCascade state. OpenCascade is not
  thread-safe, so it is reached only by message passing. One request, one response.
- **Solver** runs on the UI thread; typical sketches solve in well under a millisecond.
  Sketches above a size threshold move to a worker thread with a progress indicator.

### 4.4 Rendering

`GtkGLArea` with a depth buffer, drawn through `glow`. The renderer is defined behind a
trait so a wgpu backend can be added without touching call sites.

Picking uses an off-screen integer ID buffer rendered alongside the colour pass: each
face, edge, and vertex gets an ID, and the pixel under the cursor is read back. This
gives exact hover and selection including edges and vertices, which raycasting against
tessellation does not.

---

## 5. The document language

### 5.1 Example

```
units mm

param width  = 60
param height = 40
param wall   = 2.5

sketch base on plane.xy {
  rect r1 at (0, 0) size (width, height)
  fillet r1.corners radius 6
}

body plate = extrude(base.r1, distance = 12)

fillet f1 = fillet(
  plate.edges[ id: "e:8f2a1c", where: parallel(axis.z) and length(12) ],
  radius = 3
)

shell s1 = shell(plate, thickness = wall,
  open = plate.faces[ id: "f:2b70", where: normal(+z) ])
```

### 5.2 Language properties

- Declarative. Statements, named results, expressions, parameters, and units. No
  user-visible control flow.
- Every value may carry a unit; bare numbers use the document default declared by
  `units`.
- Parameters are expressions and may reference other parameters.
- Statement order is evaluation order, which is also the feature-tree order shown in
  the history scrubber.

### 5.3 Selectors

The syntax `[ id: "...", where: <query> ]` encodes the hybrid reference strategy
directly in the text. `id` is the kernel-persistent handle; `where` is a readable
geometric query used as a fallback and as documentation of intent.

Query predicates include `normal(dir)`, `parallel(axis)`, `perpendicular(axis)`,
`length(n)`, `area(n)`, `radius(n)`, `near(point)`, `max`/`min` over a measure, and
boolean composition with `and`, `or`, `not`.

### 5.4 Lossless round-trip

Hard requirement: the printer preserves comments, blank lines, and the user's
formatting. GUI edits patch only the AST nodes they touch, so a GUI action produces a
minimal, readable diff. Enforced by property tests asserting `print(parse(x)) == x`
across generated documents.

### 5.5 Automation

Generative and repetitive work uses embedded Rhai scripts in `scripts/`. A Rhai script
emits DSL statements; it is not itself the document. This keeps the document parseable
and patchable while still allowing loops, conditionals, and user-defined helpers.

---

## 6. Project format

A directory, so git operates on it natively:

```
Project.scriber/
  model.scr        the document
  meta.toml        schema version, default units, app version
  scripts/         user Rhai automation
  imports/         referenced STEP / STL / other imported files
  assets/          HDRIs, material definitions
  .cache/          tessellation and BREP cache — gitignored, never authoritative
```

`.cache/` is a pure derivation of `model.scr` and may be deleted at any time without
data loss. A single-file `.scrz` (zip of the directory) exists for sending a project to
someone.

### 6.1 Git integration

- A shipped `.gitattributes` plus a `textconv` filter so `git diff` renders readable
  script diffs.
- `scriber diff <a> <b>` produces a semantic, feature-level diff rather than a line diff.
- `scriber render` produces deterministic images so CI can show a visual diff of a
  design change.
- `scriber merge` resolves conflicts at feature granularity and validates that the
  merged document rebuilds before writing it.

---

## 7. Rebuild engine

### 7.1 Incremental evaluation

References between statements form a directed acyclic feature graph. An edit marks the
touched node and its descendants dirty; only that subgraph re-evaluates. Results are
memoized by a content hash of the operation and its resolved inputs, which makes undo
and dragging the rollback marker effectively instant.

The rollback marker evaluates a prefix of the document. Editing while rolled back
inserts at the marker, matching Shapr3D and Fusion behavior.

### 7.2 Selector resolution

On each rebuild, for each selector:

1. Try `id`. If it resolves to a live entity, use it.
2. Otherwise evaluate `where`. If exactly one entity matches, re-bind, and rewrite the
   `id` in the document so the reference self-heals.
3. Otherwise the feature enters the **Broken** state.

A broken feature does not destroy work. Downstream geometry from the last good build
stays visible, dimmed. A repair dialog lists candidate entities and highlights each in
the viewport as it is hovered; the user clicks the correct one and the document is
updated.

This is the single most important interaction in the application. It is where
history-based open-source CAD normally loses users, and it is the direct consequence of
choosing readable references over opaque ones.

### 7.3 Error handling

- **Parse errors** appear inline in the Script view with the offending span marked. The
  last good model stays loaded and interactive.
- **Kernel failures** — a fillet radius too large, a boolean producing nothing, a
  self-intersecting sweep — mark that feature failed, skip its dependents, and show an
  inline marker in the history scrubber plus a banner naming the failure.
- **Solver non-convergence** marks the sketch over- or under-constrained and reports
  which constraints conflict.

Nothing is ever silently dropped, and no failure discards user work.

---

## 8. User interface

### 8.1 Shell

`AdwApplicationWindow` with a thin header bar carrying an `AdwViewSwitcher`:
**Model**, **Script**, **Drawing**. Each view occupies the whole window.

### 8.2 Model view

A `GtkOverlay` whose child is the `GtkGLArea`, with three `.osd` overlays floating over
it:

| Overlay | Position | Contents |
| --- | --- | --- |
| Tool strip | left, vertical | Sketch, Extrude, Fillet, Chamfer, Shell, Loft, Sweep, Pattern, Boolean, Measure. Icon-only; sub-tools open in a `GtkPopover`. |
| Tool properties | right | Visible only while a tool is active. Numeric entries with unit parsing, direction and operation dropdowns, Done and Cancel. |
| History scrubber | bottom | Horizontal feature timeline with a draggable rollback marker. Clicking a feature edits it in place. Failed and broken features are marked here. |

A view cube sits in a corner as a fourth, always-present `.osd` element.

### 8.3 Adaptive tool prediction

The tool strip reorders its top slots based on the current selection:

| Selection | Promoted tools |
| --- | --- |
| Edge | Fillet, Chamfer |
| Face | Push-pull, Offset face, Shell |
| Sketch profile | Extrude, Revolve |
| Body | Boolean, Shell, Pattern |
| Two bodies | Union, Subtract, Intersect, Align |
| Nothing | Sketch, Import, Measure |

Ranking is a small scored table, not a learned model. It is deterministic, unit-testable,
and user-overridable by pinning tools.

### 8.4 Viewport interaction

Middle-drag orbits, Shift plus middle-drag pans, scroll zooms. Hovering highlights the
entity under the cursor using the ID buffer. Push-pull uses an on-canvas gizmo. Escape
cancels the active tool; Enter commits. Typing digits during a drag opens an inline
numeric entry, preserving Shapr3D and Fusion muscle memory.

### 8.5 Touch and pen

`GtkGestureZoom`, `GtkGestureRotate`, and `GtkGestureDrag` feed the same state machines
as the pointer. A stylus draws directly in sketch mode. Touch does not change the layout.

### 8.6 Theming

Adwaita light and dark following the system preference, with the accent colour taken
from GNOME settings. No custom palette. Icons drawn fresh in the Adwaita style.

---

## 9. Testing

| Crate | Approach |
| --- | --- |
| `scriber-lang` | Property tests for `print(parse(x)) == x` over generated documents; parser error-recovery cases |
| `scriber-solver` | Golden constraint systems; DOF-count assertions; convergence, over-constrained, and under-constrained detection |
| `scriber-kernel` | Per-operation volume, area, and topology assertions; STEP export-import round-trip |
| `scriber-doc` | Rebuild determinism; **selector-survival suite** — mutate an upstream feature, assert every downstream selector still resolves to the same entity |
| `scriber-render` | Deterministic image comparison of known scenes |
| `scriber-ui` | Headless tests over tool state machines; viewport screenshot tests rendered through `scriber-cli` |

A corpus of `.scr` documents is rebuilt on every CI run. Any document that breaks
becomes a permanent regression test.

---

## 10. Build order

Each milestone is independently shippable and testable.

| M | Deliverable |
| --- | --- |
| 0 | Workspace skeleton, Flatpak manifest building OCCT 8.0.1 as a shared module, self-hosted repo publishing pipeline, CI. `scriber-occt` bridge bootstrapped and proven end to end with a box-minus-cylinder boolean exported to STEP. |
| 1 | `scriber-lang` + `scriber-doc` + `scriber-cli`. Headless: document in, STEP and STL out. No GUI. |
| 2 | GTK4 shell, `GtkGLArea` viewport, tessellation, ID-buffer picking, camera, view cube. |
| 3 | `scriber-solver`, sketcher UI, auto-constrain, fully-defined indicator. |
| 4 | Core features — extrude, push-pull, revolve, boolean, fillet, chamfer, shell — plus the selector repair UI. |
| 5 | Adaptive tool strip, history scrubber, rollback, variables, Script view. |
| 6 | Remaining features (loft, sweep, draft, patterns, wrap, emboss) and full import/export breadth. |
| 7 | Materials, HDRI environment, section views, measure, exploded view. |
| 8 | 2D drawings module. |
| 9 | Git tooling — textconv, `scriber diff`, feature-level merge helper. |

Milestone 1 is the first point at which the product's distinguishing idea is real and
usable. Milestone 4 is the first point at which it is recognisably a CAD application.

---

## 11. Known risks

| Risk | Mitigation |
| --- | --- |
| Writing and maintaining our own OCCT bridge is ongoing work | Wrap only what the current milestone needs; never wrap speculatively. All unsafe FFI is confined to `scriber-occt` so the rest of the codebase stays safe Rust. OCCT's C++ API is stable across patch releases, so churn is low. `opencascade-rs` remains available to read as a reference for how a given OCCT call is bridged. |
| OCCT must be dynamically linked to stay within the LGPL exception | The Flatpak manifest builds OCCT 8.0.1 as a shared library module; `scriber-occt`'s build script links against it rather than embedding it. A CI check asserts the produced binary has a dynamic `libTKernel` dependency and no statically embedded OCCT objects. |
| Topological naming is an unsolved problem in open-source CAD | The hybrid selector plus an explicit repair UI accepts that automatic resolution will sometimes fail, and makes failure recoverable rather than silent. This is a UX answer to a problem with no clean technical answer. |
| Writing a constraint solver is substantial work | Well-documented mathematics with published references. Scoped to 2D; 3D constraints are out of scope. |
| Lossless round-trip constrains DSL design permanently | Establish the property test in M1, before any GUI depends on the printer. |
| GTK4 has no wgpu path | Ship on `GtkGLArea` and OpenGL, renderer behind a trait. Revisit only if GTK upstream resolves it. |
| Scope is very large | Milestones are ordered so each one is independently useful. Section 2.3 is enforced, not aspirational. |
| Self-hosting distribution means no Flathub discovery, and users must trust a third-party remote | Sign the repository with GPG and publish the fingerprint in the README. Ship a `.flatpakref` so adding the remote is one click, and a standalone bundle for users who prefer not to add a remote at all. Reproducible manifest so anyone can rebuild and compare. |

---

## 12. Distribution

Scriber is not published to Flathub. It ships from a Flatpak repository the project
hosts itself, which users add as a remote.

### 12.1 Repository

The build produces an OSTree repository published as static files on GitHub Pages at
`https://<owner>.github.io/scriber/`:

```
scriber/
  repo/                     OSTree repository (the actual Flatpak remote)
  scriber.flatpakrepo       remote definition — URL, GPG key, title, description
  scriber.flatpakref        one-click install reference for the app
  scriber.gpg               exported public signing key
```

The repository is GPG-signed. The public key fingerprint is published in the README so
users can verify what they are trusting before adding the remote.

### 12.2 Install instructions for the README

Primary path — add the remote once, then install and receive updates through
`flatpak update` like any other application:

```sh
flatpak remote-add --if-not-exists scriber \
  https://<owner>.github.io/scriber/scriber.flatpakrepo

flatpak install scriber io.github.<owner>.Scriber
```

One-click alternative — the `.flatpakref` adds the remote and installs in a single step,
and is what the download button on the project page points at:

```sh
flatpak install https://<owner>.github.io/scriber/scriber.flatpakref
```

Fallback for users who would rather not add a remote — a self-contained bundle attached
to each GitHub release. This installs a fixed version and does **not** receive automatic
updates:

```sh
flatpak install --bundle Scriber-<version>-x86_64.flatpak
```

Building from source is documented separately in the README for contributors; it is not
the recommended install path.

### 12.3 Release pipeline

A GitHub Actions workflow on tag:

1. `flatpak-builder` builds the manifest for `x86_64` and `aarch64`.
2. `flatpak build-export` writes the build into `repo/`, GPG-signed.
3. `flatpak build-update-repo --generate-static-deltas --prune` updates repository
   metadata and keeps download sizes small.
4. `flatpak build-bundle` produces the standalone `.flatpak` for the release page.
5. `repo/` and the `.flatpakrepo` / `.flatpakref` / `.gpg` files are published to the
   GitHub Pages branch.

The signing key lives in Actions secrets. The manifest pins every dependency to a commit
or tarball hash so a build is reproducible and independently verifiable.
