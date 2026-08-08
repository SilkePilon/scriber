# Scriber Milestone 1 — The Language

**Date:** 2026-08-08
**Status:** Approved design, pending implementation plan
**Depends on:** `docs/superpowers/specs/2026-08-07-scriber-design.md` (the overall design)
**Milestone 0:** shipped as v0.1.0

---

## 1. Summary

Milestone 1 builds `scriber-lang`: the language a Scriber document is written in. A document parses, type-checks, evaluates, and produces geometry through the kernel built in Milestone 0.

This milestone settles the requirement the whole product rests on — that a document can be parsed and reprinted **byte for byte**, so the GUI can later patch one statement without disturbing anyone's formatting. Everything downstream assumes it, so it is proved first.

### Why the language before the document model

The overall design lists Milestone 1 as `scriber-lang` + `scriber-doc` + CLI. That is two large subsystems, and the second depends on decisions the first makes. The feature DAG, incremental rebuild, and selector resolution move to their own milestone.

### What the language can express

The DSL is bounded by what `scriber-kernel` actually has. That is a deliberate constraint, not an oversight.

| In scope | Out of scope, and why |
| --- | --- |
| `units`, `param`, expressions with dimension checking | sketches — need the Milestone 3 constraint solver |
| `cuboid`, `cylinder`, `cut` | `extrude`, `revolve`, `fillet`, `shell` — need Milestone 4 kernel work |
| `export` to STEP and STL | **selectors** — the kernel cannot enumerate faces or edges at all, so a selector could only parse and then fail |

Selectors are the notable omission. The overall design's headline example uses
`plate.edges[ id: "e:8f2a1c", where: parallel(axis.z) ]`, and none of that is
reachable: there is no topology enumeration in the bridge. Designing a query
language against an API that does not exist is how you design it wrong, so the
selector grammar is deferred to the milestone that makes it real.

### The one kernel addition

The overall design promises "STEP and STL out" for this milestone, but the bridge has only `write_step`. `write_stl` is added: an `StlAPI_Writer` shim of roughly fifteen lines, routed through the existing `guard()` like every other shim function, with a paired test. This is the single exception to "language only".

`scriber smoke` is retired once `scriber build` works. It was a diagnostic proving the M0 pipeline, not a feature.

---

## 2. Decisions

| Decision | Choice | Why |
| --- | --- | --- |
| Syntax tree | `rowan` lossless CST, typed AST as a view over it | The red-green tree rust-analyzer is built on. Whitespace and comments live in the tree as trivia, so `print(parse(x)) == x` holds **by construction** rather than by care. Supports incremental reparse when the GUI arrives. Cost: a two-layer model, and every AST accessor returns `Option`. |
| Dimensions | Three — length, angle, scalar — checked | `cylinder(radius = 45deg, height = 10mm)` should not build silently wrong geometry. Three dimensions catch the mistakes that actually happen without building a physics library. |
| Derived dimensions | Rejected in M1 | Nothing in M1 or M4 consumes an area or a volume as an input. Unit algebra decided now, with no consumer to check it against, would be decided wrong. Loosening later is compatible; tightening would not be. |
| Error handling | Recovering parser, all diagnostics in one pass | Milestone 5's Script view needs squiggles on every bad line, not just the first. `rowan` holds error nodes natively, so recovery is structural. Retrofitting recovery into a parser written to bail is a parser rewrite. |
| Geometry access | `Backend` trait, implemented outside `scriber-lang` | Keeps the entire language test suite free of OCCT — no C++ compile, no linking — so property tests over generated documents run thousands of cases fast. Also enables a recording backend that asserts *which* kernel calls a document produced. |
| Selectors | Excluded from the grammar | See above. |
| Diagnostics rendering | `codespan-reporting` | Source excerpt, caret span, message. Same output serves the CLI now and the Script view later. |

---

## 3. The language

### 3.1 Worked example

```
# A plate with a bore through it.
units mm

param width  = 60
param height = 40
param wall   = 2.5mm
param bore   = width / 6

body plate = cuboid(width, height, 12)
body hole  = cylinder(radius = bore, height = 12)
body part  = cut(plate, hole)

export "part.step" from part
export "part.stl"  from part
```

### 3.2 Grammar

```ebnf
document    = { statement } ;

statement   = units_decl | param_decl | body_decl | export_stmt ;
units_decl  = "units" , length_unit ;
param_decl  = "param" , ident , "=" , expr ;
body_decl   = "body" , ident , "=" , expr ;
export_stmt = "export" , string , [ "from" , ident ] ;

expr        = term , { ( "+" | "-" ) , term } ;
term        = factor , { ( "*" | "/" ) , factor } ;
factor      = [ "-" ] , primary ;
primary     = number , [ unit ]
            | ident
            | call
            | "(" , expr , ")" ;
call        = ident , "(" , [ arg , { "," , arg } , [ "," ] ] , ")" ;
arg         = [ ident , "=" ] , expr ;

length_unit = "mm" | "cm" | "m" | "in" | "ft" ;
angle_unit  = "deg" | "rad" ;
unit        = length_unit | angle_unit ;
comment     = "#" , { ? any character except newline ? } ;
```

Binary operators are left-associative. Precedence, loosest first: `+` `-`, then `*` `/`, then unary `-`. Comments and newlines are trivia: they carry no meaning but are preserved in the CST.

Trailing commas in argument lists are permitted, because the GUI will generate them.

### 3.3 Statements

**`units`** declares the default length unit for the document. At most one, and it must precede any statement that relies on it. Absent, the default is millimetres.

**`param`** binds a name to a length, angle or scalar; binding geometry to a `param` is an error. **`body`** binds a name to geometry; binding a non-geometry value to a `body` is an error. The two keywords therefore also serve as the reader's type annotation. Both share one namespace, so a `param` and a `body` cannot have the same name.

Evaluation is strictly top to bottom, and **forward references are errors**. Statement order *is* the model's history — that is what makes the document a feature tree — so a name must be declared before use.

Redeclaring a name is an error.

**`export`** writes a body to a path, resolved relative to the document's own directory. Format comes from the extension: `.step`/`.stp` produce STEP, `.stl` produces STL; anything else is an error. The `from` clause names the body; if omitted, the document must contain exactly one `body`, otherwise it is an error naming the candidates.

### 3.4 Dimensions

Every value has one of three dimensions: **length**, **angle**, or **scalar**. Literals may carry a unit — `12mm`, `1.5in`, `45deg`, `0.4rad`. A bare number is a scalar.

Internally, lengths normalize to millimetres and angles to radians. Conversion happens at the literal; the rest of the evaluator sees normalized values.

**Arithmetic**, where L is length, A angle, S scalar:

| Operation | Result |
| --- | --- |
| `L + L`, `L - L` | L |
| `A + A`, `A - A` | A |
| `S + S`, `S - S` | S |
| `L + S`, `S + L`, `L - S`, `S - L` | L — the scalar is read in the document's units |
| `A + S` and any other mixed additive pair | error |
| `L * S`, `S * L` | L |
| `A * S`, `S * A` | A |
| `S * S` | S |
| `L * L`, `L * A`, `A * A` | error — no operation consumes an area |
| `L / S` | L |
| `L / L`, `A / A`, `S / S` | S |
| `A / S` | A |
| `S / L`, `S / A` | error |
| unary `-` | preserves dimension |

The scalar-to-length coercion in additive position deserves a note. `units mm` declares a default **length** unit, so `wall + 1` reads the `1` as one millimetre. There is no declared default angle unit, so `angle + 1` is an error and angles always need explicit units. The asymmetry is deliberate.

Division by zero is an evaluation error, not an infinity.

### 3.5 Coercion at call boundaries

A parameter declared as a length accepts a scalar, read in document units. So `cuboid(width, height, 12)` means twelve millimetres for the third argument. A parameter declared as a length does **not** accept an angle.

### 3.6 Builtin functions

| Function | Signature |
| --- | --- |
| `sqrt(S)` | S |
| `abs(x)` | same dimension as `x` |
| `min(x, y)`, `max(x, y)` | both arguments must share a dimension; if one is a length and the other a scalar, the scalar is read in document units and the result is a length. Mixing a length or scalar with an angle is an error |
| `floor(S)`, `ceil(S)` | S |
| `sin(A)`, `cos(A)`, `tan(A)` | S |

`sqrt` on a length is rejected, since its result would be a derived dimension.

### 3.7 Builtin geometry

| Operation | Signature |
| --- | --- |
| `cuboid(dx: L, dy: L, dz: L)` | Body — one corner at the origin |
| `cylinder(radius: L, height: L)` | Body — axis along +Z, base at the origin |
| `cut(target: Body, tool: Body)` | Body |

Arguments may be positional or named. Named form is what the GUI emits later.

`volume()` is deliberately **not** in the language: its result is a length cubed, which the dimension rules reject, and it is a query rather than a modelling step. It is exposed as a CLI command instead.

---

## 4. Architecture

### 4.1 Crate layout

```
crates/scriber-lang/
  src/lexer.rs    tokens, with whitespace and comments as trivia
  src/syntax.rs   SyntaxKind and the rowan Language impl
  src/parser.rs   recovering parser, produces a CST with error nodes
  src/ast.rs      typed views over the CST
  src/units.rs    Dimension, Quantity, unit parsing and conversion
  src/eval.rs     evaluates a document against a Backend
  src/diag.rs     Diagnostic and its rendering
  src/print.rs    CST back to text
```

`print.rs` is close to trivial: printing is concatenating the tree's tokens in order. Losslessness is a property of the representation, and the property test guards it rather than establishing it.

### 4.2 Pipeline

```
source text
  -> lexer      tokens + trivia
  -> parser     rowan CST (may contain error nodes)
  -> ast        typed views
  -> eval       + Backend  ->  geometry
```

One direction. No stage reaches backwards.

### 4.3 The Backend seam

`scriber-lang` does not depend on `scriber-kernel`. It defines what it needs:

```rust
pub trait Backend {
    type Body;

    fn cuboid(&mut self, dx: Length, dy: Length, dz: Length)
        -> Result<Self::Body, String>;

    fn cylinder(&mut self, radius: Length, height: Length)
        -> Result<Self::Body, String>;

    fn cut(&mut self, target: &Self::Body, tool: &Self::Body)
        -> Result<Self::Body, String>;

    fn export(&mut self, body: &Self::Body, path: &Path)
        -> Result<(), String>;
}
```

`scriber-cli` implements it over `scriber-kernel`. Two consequences, both wanted:

1. The language's test suite needs no OCCT — no C++ compilation, no linking — so property tests can run thousands of generated documents.
2. A recording backend can assert the exact sequence of kernel calls a document produces, which is a sharper test than comparing volumes.

The trait will grow every time the kernel does. That churn is accepted; it is the price of the seam.

Backend errors are strings, which `eval` wraps in a diagnostic carrying the offending statement's span. The language does not model kernel error structure.

---

## 5. Errors

The parser recovers at statement boundaries and reports every syntax error in one pass. The evaluator then accumulates every error it can before stopping.

| Class | Examples |
| --- | --- |
| Lexical | unterminated string, unknown character, malformed number |
| Syntax | missing `=`, unclosed paren, unexpected token |
| Name | unknown name, use before declaration, duplicate declaration |
| Dimension | `12mm + 45deg`, `length * length`, `sqrt` of a length |
| Call | wrong arity, unknown function, unknown named argument, duplicate argument |
| Unit | unknown unit suffix, `units` declared twice or after use |
| Export | unknown extension, no `from` with zero or several bodies |
| Kernel | any `Backend` failure, with the statement's span attached |

Diagnostics carry a severity, a primary span, an optional secondary span (a duplicate declaration points at the original), and a message. Rendering is `codespan-reporting`.

A document with any error produces **no** geometry and **no** files. Partial output would be worse than none, since a stale file that looks fresh is a trap. Milestone 5's Script view keeps the last good model in memory instead, but that is a document-model concern, not a language one.

---

## 6. CLI

| Command | Behaviour |
| --- | --- |
| `scriber build <doc.scr>` | evaluate and run the document's `export` statements |
| `scriber export <doc.scr> -o <file> [--body <name>]` | ad-hoc export, format from the extension. Unlike a document's own `export`, `-o` is resolved relative to the current working directory, since it is the caller speaking rather than the document. The document's `export` statements are **not** run |
| `scriber check <doc.scr>` | parse and type-check only; no geometry, no OCCT touched |
| `scriber fmt <doc.scr> [--check]` | parse and reprint; `--check` exits non-zero if output differs |
| `scriber volume <doc.scr> [--body <name>]` | print a body's volume |

`scriber fmt` is the round-trip property exposed as a tool. On a correct implementation it never changes a file, which makes it a useful canary in CI.

---

## 7. Testing

| Kind | What it covers |
| --- | --- |
| Property | `print(parse(x)) == x` over generated documents; `parse(print(t))` tree-equal to `t` |
| Recording backend | the exact sequence of kernel calls a document produces, without OCCT |
| Golden diagnostics | `insta` snapshots of rendered errors, so message quality is version-controlled |
| Unit | dimension algebra table, unit conversion, precedence and associativity |
| Corpus | `.scr` files under `tests/corpus`, evaluated every CI run; anything that breaks becomes a regression test |
| Integration | a small number of `scriber-cli` tests that produce a real STEP and STL through OCCT |

The property test is the milestone's centrepiece. Its generator must produce comments, blank lines, unusual spacing, and trailing commas — the things a naive printer destroys — or it proves nothing.

---

## 8. Risks

| Risk | Mitigation |
| --- | --- |
| `rowan`'s two-layer model is unfamiliar and every accessor returns `Option` | Confine it to `syntax.rs` and `ast.rs`; the rest of the crate sees typed views |
| A selector grammar designed later may not fit the expression grammar | Adding node kinds to `rowan` is cheap. The alternative was designing a query language against a topology API that does not exist |
| The `Backend` trait grows with every kernel addition | Accepted. It is what keeps language tests OCCT-free |
| Dimension rules may prove too strict | Revisit when an operation consumes an area. Loosening is backward-compatible; tightening would not be |
| The property test generator may be too tame to prove anything | Generate trivia deliberately: comments, blank runs, odd spacing, trailing commas. Review the generator as carefully as the printer |
| Scalar-to-length coercion may surprise users | It applies only where a length is expected and in additive position with a length. Documented, and covered by golden diagnostics for the rejected cases |

---

## 9. Out of scope

Feature DAG, incremental rebuild, memoization, rollback, undo as transactions, selector resolution and repair, the project directory format, git tooling, and any GUI. Each belongs to a later milestone and gets its own spec.
