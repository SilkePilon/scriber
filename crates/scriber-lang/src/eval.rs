//! Evaluates a document against a [`Backend`].
//!
//! Two passes. The first resolves names, checks dimensions and validates
//! calls, collecting every diagnostic it can. Only if that pass is clean does
//! the second pass run geometry — so a document with any error produces no
//! files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rowan::TextRange;

use crate::ast::{Document, Expr, Stmt};
use crate::backend::Backend;
use crate::diag::Diagnostic;
use crate::parse;
use crate::syntax::SyntaxKind;
use crate::units::{Dimension, LengthUnit, Quantity, UnitError, parse_number};

/// A bound value: either a number with a dimension, or geometry.
#[derive(Debug, Clone)]
pub enum Value<B> {
    Quantity(Quantity),
    Body(B),
}

/// Every binding a document produced, in declaration order.
#[derive(Debug)]
pub struct Evaluated<B> {
    pub values: Vec<(String, Value<B>)>,
}

/// Parses and evaluates `source`.
///
/// `base_dir` is what a document's own `export` paths resolve against.
pub fn evaluate<B: Backend>(
    source: &str,
    base_dir: &Path,
    backend: &mut B,
) -> Result<Evaluated<B::Body>, Vec<Diagnostic>> {
    let parsed = parse(source);
    // Syntax errors come first and are never skipped. `Document::statements`
    // drops `Error` nodes, so a document nothing could be made of looks empty
    // from up here; folding the parser's own errors in is what keeps that from
    // reading as success.
    let mut diagnostics: Vec<Diagnostic> = parsed
        .errors
        .iter()
        .cloned()
        .map(Diagnostic::from)
        .collect();

    let Some(document) = Document::cast(parsed.syntax()) else {
        diagnostics.push(Diagnostic::error(
            "not a document",
            TextRange::new(0.into(), 0.into()),
        ));
        return Err(diagnostics);
    };

    let mut evaluator = Evaluator {
        units: LengthUnit::Mm,
        units_declared: false,
        scope: HashMap::new(),
        order: Vec::new(),
        diagnostics,
        base_dir: base_dir.to_path_buf(),
    };

    evaluator.check(&document);

    if !evaluator.diagnostics.is_empty() {
        return Err(evaluator.diagnostics);
    }

    evaluator.run(&document, backend)
}

/// What a name was bound to, without the geometry itself.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Quantity(Dimension),
    Body,
}

/// A stand-in operand used to ask the dimension rules a question without
/// having a value to hand.
///
/// The value is deliberately `1.0`: it is finite, and non-zero so that the
/// division rule's zero guard cannot fire on a placeholder.
fn probe(dimension: Dimension) -> Quantity {
    Quantity {
        value: 1.0,
        dimension,
    }
}

struct Evaluator {
    units: LengthUnit,
    units_declared: bool,
    scope: HashMap<String, (Kind, TextRange)>,
    order: Vec<String>,
    diagnostics: Vec<Diagnostic>,
    base_dir: PathBuf,
}

impl Evaluator {
    fn error(&mut self, message: impl Into<String>, range: TextRange) {
        self.diagnostics.push(Diagnostic::error(message, range));
    }

    // ---- pass one: names, dimensions, calls -----------------------------

    fn check(&mut self, document: &Document) {
        for stmt in document.statements() {
            match stmt {
                Stmt::Units(decl) => {
                    let range = decl.range();
                    let Some(token) = decl.unit() else { continue };

                    if self.units_declared {
                        self.error("`units` is already declared", range);
                    } else if !self.order.is_empty() {
                        self.error("`units` must come before any other statement", range);
                    }

                    match LengthUnit::parse(token.text()) {
                        Some(unit) => self.units = unit,
                        None => self.error(
                            format!("`{}` is not a length unit", token.text()),
                            token.text_range(),
                        ),
                    }
                    self.units_declared = true;
                }

                Stmt::Param(decl) => {
                    let range = decl.range();
                    let Some(name) = decl.name() else { continue };
                    let Some(value) = decl.value() else { continue };

                    let kind = self.check_expr(&value);
                    if let Some(Kind::Body) = kind {
                        self.error("a `param` cannot hold geometry — use `body`", range);
                    }
                    self.declare(
                        name.text(),
                        kind.unwrap_or(Kind::Quantity(Dimension::Scalar)),
                        name.text_range(),
                    );
                }

                Stmt::Body(decl) => {
                    let range = decl.range();
                    let Some(name) = decl.name() else { continue };
                    let Some(value) = decl.value() else { continue };

                    let kind = self.check_expr(&value);
                    if let Some(Kind::Quantity(_)) = kind {
                        self.error("a `body` must hold geometry — use `param`", range);
                    }
                    self.declare(name.text(), Kind::Body, name.text_range());
                }

                Stmt::Export(stmt) => self.check_export(&stmt),
            }
        }
    }

    fn declare(&mut self, name: &str, kind: Kind, range: TextRange) {
        if let Some((_, previous)) = self.scope.get(name) {
            let previous = *previous;
            self.diagnostics.push(
                Diagnostic::error(format!("`{name}` is already declared"), range)
                    .with_note("first declared here", previous),
            );
            return;
        }

        self.scope.insert(name.to_string(), (kind, range));
        self.order.push(name.to_string());
    }

    fn check_export(&mut self, stmt: &crate::ast::ExportStmt) {
        let range = stmt.range();
        let Some(path_token) = stmt.path() else {
            return;
        };
        let path = path_token.text().trim_matches('"').to_string();

        if extension_format(&path).is_none() {
            self.error(
                format!("cannot export `{path}` — expected a .step, .stp or .stl extension"),
                path_token.text_range(),
            );
        }

        match stmt.body() {
            Some(name) => {
                if !self.scope.contains_key(name.text()) {
                    self.error(
                        format!("`{}` is not declared", name.text()),
                        name.text_range(),
                    );
                } else if !matches!(self.scope[name.text()].0, Kind::Body) {
                    self.error(
                        format!("`{}` is not a body", name.text()),
                        name.text_range(),
                    );
                }
            }
            None => {
                // Counted at this point in the document, not at the end, so the
                // answer matches the one pass two reaches when it resolves the
                // same `export` in statement order.
                let bodies: Vec<String> = self
                    .order
                    .iter()
                    .filter(|name| matches!(self.scope[*name].0, Kind::Body))
                    .cloned()
                    .collect();

                if bodies.len() != 1 {
                    let named = if bodies.is_empty() {
                        "none".to_string()
                    } else {
                        bodies
                            .iter()
                            .map(|name| format!("`{name}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    self.error(
                        format!(
                            "`export` needs `from <name>` to say which body — the document declares {named}"
                        ),
                        range,
                    );
                }
            }
        }
    }

    /// Returns the kind an expression produces, or `None` if it was invalid.
    fn check_expr(&mut self, expr: &Expr) -> Option<Kind> {
        match expr {
            Expr::Literal(literal) => {
                let token = literal.token()?;
                match parse_number(token.text(), self.units) {
                    Ok(quantity) => Some(Kind::Quantity(quantity.dimension)),
                    Err(UnitError::UnknownUnit(unit)) => {
                        self.error(format!("`{unit}` is not a unit"), token.text_range());
                        None
                    }
                    Err(UnitError::MalformedNumber(text)) => {
                        self.error(format!("`{text}` is not a number"), token.text_range());
                        None
                    }
                }
            }

            Expr::Name(name) => {
                let token = name.token()?;
                match self.scope.get(token.text()) {
                    Some((kind, _)) => Some(*kind),
                    None => {
                        self.error(
                            format!(
                                "`{}` is not declared — names must be declared before use",
                                token.text()
                            ),
                            token.text_range(),
                        );
                        None
                    }
                }
            }

            Expr::Paren(paren) => self.check_expr(&paren.inner()?),

            Expr::Unary(unary) => match self.check_expr(&unary.operand()?)? {
                Kind::Quantity(dimension) => Some(Kind::Quantity(dimension)),
                // Same message pass two would give, moved forward so it is
                // reported alongside the document's other errors.
                Kind::Body => {
                    self.error("cannot negate geometry", expr.range());
                    None
                }
            },

            Expr::Bin(bin) => {
                let lhs = self.check_expr(&bin.lhs()?)?;
                let rhs = self.check_expr(&bin.rhs()?)?;
                let op = bin.op()?;

                let (Kind::Quantity(a), Kind::Quantity(b)) = (lhs, rhs) else {
                    self.error("arithmetic needs numbers, not geometry", expr.range());
                    return None;
                };

                // Probe the dimension rules with value 1.0 on both sides. The
                // denominator is deliberately non-zero: division by zero is a
                // runtime concern, not a dimension error, and must not be
                // reported here where the operands are placeholders.
                let result = match op {
                    SyntaxKind::Plus => probe(a).add(probe(b), self.units),
                    SyntaxKind::Minus => probe(a).sub(probe(b), self.units),
                    SyntaxKind::Star => probe(a).mul(probe(b), self.units),
                    SyntaxKind::Slash => probe(a).div(probe(b), self.units),
                    _ => Ok(probe(a)),
                };

                match result {
                    Ok(quantity) => Some(Kind::Quantity(quantity.dimension)),
                    Err(error) => {
                        self.error(error.message, expr.range());
                        None
                    }
                }
            }

            Expr::Call(call) => {
                let callee = call.callee()?;
                let args: Vec<crate::ast::Arg> = call.args().collect();
                let kinds: Vec<Option<Kind>> = args
                    .iter()
                    .map(|arg| arg.value().and_then(|e| self.check_expr(&e)))
                    .collect();

                let Some(signature) = signature_of(callee.text()) else {
                    self.error(
                        format!("`{}` is not a known function", callee.text()),
                        callee.text_range(),
                    );
                    return None;
                };

                if args.len() != signature.params.len() {
                    self.error(
                        format!(
                            "`{}` expects {} argument{}, found {}",
                            callee.text(),
                            signature.params.len(),
                            if signature.params.len() == 1 { "" } else { "s" },
                            args.len()
                        ),
                        call.range(),
                    );
                    return None;
                }

                // Every argument is checked before bailing, so one bad argument
                // does not hide the next.
                let mut sound = true;
                for (index, (arg, kind)) in args.iter().zip(&kinds).enumerate() {
                    // A name labels its position, it does not move the argument
                    // to one: pass two reads arguments by index and never looks
                    // at the names. Letting `cylinder(height = 5, radius = 12)`
                    // through would build a 5mm-radius cylinder from a document
                    // that says 12 — wrong geometry, with nothing reported.
                    if let Some(name) = arg.name() {
                        if !signature.params.iter().any(|param| *param == name.text()) {
                            self.error(
                                format!("`{}` has no parameter `{}`", callee.text(), name.text()),
                                name.text_range(),
                            );
                            sound = false;
                        } else if let Some(expected) = signature.params.get(index)
                            && *expected != name.text()
                        {
                            self.error(
                                format!(
                                    "`{}` takes `{expected}` in this position, not `{}` — a named argument labels its position rather than moving it",
                                    callee.text(),
                                    name.text()
                                ),
                                name.text_range(),
                            );
                            sound = false;
                        }
                    }

                    let Some(kind) = *kind else {
                        sound = false;
                        continue;
                    };

                    if let Err(message) = signature.wants.check(kind, self.units) {
                        self.error(message, arg.range());
                        sound = false;
                    }
                }

                if !sound {
                    return None;
                }

                self.result_of(&signature, &kinds, call.range())
            }
        }
    }

    /// The kind a call produces, worked out the same way pass two works out the
    /// value's dimension.
    fn result_of(
        &mut self,
        signature: &Signature,
        kinds: &[Option<Kind>],
        range: TextRange,
    ) -> Option<Kind> {
        let argument = |index: usize| match kinds.get(index).copied().flatten() {
            Some(Kind::Quantity(dimension)) => Some(dimension),
            _ => None,
        };

        match signature.result {
            ResultOf::Body => Some(Kind::Body),
            ResultOf::Scalar => Some(Kind::Quantity(Dimension::Scalar)),
            // `abs` hands back the dimension it was given. Claiming a scalar
            // here would reject `abs(-45deg) + 45deg`, which pass two evaluates
            // happily — a pass-one error for a document that has none.
            ResultOf::SameAsArgument => Some(Kind::Quantity(argument(0)?)),
            // `min`/`max` unify their arguments with the additive rules, which
            // is exactly what pass two does.
            ResultOf::UnifiedArguments => {
                let (a, b) = (argument(0)?, argument(1)?);
                match probe(a).add(probe(b), self.units) {
                    Ok(quantity) => Some(Kind::Quantity(quantity.dimension)),
                    Err(error) => {
                        self.error(error.message, range);
                        None
                    }
                }
            }
        }
    }

    // ---- pass two: geometry ---------------------------------------------

    fn run<B: Backend>(
        &self,
        document: &Document,
        backend: &mut B,
    ) -> Result<Evaluated<B::Body>, Vec<Diagnostic>> {
        let mut values: HashMap<String, Value<B::Body>> = HashMap::new();
        let mut order: Vec<(String, Value<B::Body>)> = Vec::new();
        let mut errors: Vec<Diagnostic> = Vec::new();
        let mut exports: Vec<(String, PathBuf, TextRange)> = Vec::new();

        for stmt in document.statements() {
            let bound = match &stmt {
                Stmt::Param(decl) => decl.name().zip(decl.value()),
                Stmt::Body(decl) => decl.name().zip(decl.value()),
                _ => None,
            };

            if let Some((name, expr)) = bound {
                match self.eval_expr(&expr, &values, backend) {
                    Ok(value) => {
                        values.insert(name.text().to_string(), value.clone());
                        order.push((name.text().to_string(), value));
                    }
                    Err(diagnostic) => errors.push(diagnostic),
                }
                continue;
            }

            if let Stmt::Export(export) = &stmt {
                let Some(path_token) = export.path() else {
                    continue;
                };
                let path = path_token.text().trim_matches('"');
                let target = self.base_dir.join(path);

                // Resolved here rather than after the loop so that an `export`
                // with no `from` names the body that was in scope where it was
                // written, which is the one pass one counted.
                let body_name = match export.body() {
                    Some(name) => name.text().to_string(),
                    None => order
                        .iter()
                        .rev()
                        .find(|(_, value)| matches!(value, Value::Body(_)))
                        .map(|(name, _)| name.clone())
                        .unwrap_or_default(),
                };

                exports.push((body_name, target, export.range()));
            }
        }

        // Files are written only once every binding has succeeded. Exporting
        // and then failing would leave an artefact that looks fresh but is not,
        // which is worse than leaving none.
        if errors.is_empty() {
            for (body_name, target, range) in exports {
                match values.get(&body_name) {
                    Some(Value::Body(body)) => {
                        if let Err(message) = backend.export(body, &target) {
                            errors.push(Diagnostic::error(message, range));
                        }
                    }
                    _ => errors.push(Diagnostic::error(
                        format!("`{body_name}` is not a body"),
                        range,
                    )),
                }
            }
        }

        if errors.is_empty() {
            Ok(Evaluated { values: order })
        } else {
            Err(errors)
        }
    }

    fn eval_expr<B: Backend>(
        &self,
        expr: &Expr,
        scope: &HashMap<String, Value<B::Body>>,
        backend: &mut B,
    ) -> Result<Value<B::Body>, Diagnostic> {
        match expr {
            Expr::Literal(literal) => {
                let token = literal.token().ok_or_else(|| bad(expr))?;
                let quantity = parse_number(token.text(), self.units).map_err(|_| bad(expr))?;
                Ok(Value::Quantity(quantity))
            }

            Expr::Name(name) => {
                let token = name.token().ok_or_else(|| bad(expr))?;
                scope.get(token.text()).cloned().ok_or_else(|| bad(expr))
            }

            Expr::Paren(paren) => {
                self.eval_expr(&paren.inner().ok_or_else(|| bad(expr))?, scope, backend)
            }

            Expr::Unary(unary) => {
                let inner =
                    self.eval_expr(&unary.operand().ok_or_else(|| bad(expr))?, scope, backend)?;
                match inner {
                    Value::Quantity(quantity) => Ok(Value::Quantity(quantity.neg())),
                    Value::Body(_) => {
                        Err(Diagnostic::error("cannot negate geometry", expr.range()))
                    }
                }
            }

            Expr::Bin(bin) => {
                let lhs = self.eval_expr(&bin.lhs().ok_or_else(|| bad(expr))?, scope, backend)?;
                let rhs = self.eval_expr(&bin.rhs().ok_or_else(|| bad(expr))?, scope, backend)?;
                let op = bin.op().ok_or_else(|| bad(expr))?;

                let (Value::Quantity(a), Value::Quantity(b)) = (lhs, rhs) else {
                    return Err(Diagnostic::error(
                        "arithmetic needs numbers, not geometry",
                        expr.range(),
                    ));
                };

                let result = match op {
                    SyntaxKind::Plus => a.add(b, self.units),
                    SyntaxKind::Minus => a.sub(b, self.units),
                    SyntaxKind::Star => a.mul(b, self.units),
                    SyntaxKind::Slash => a.div(b, self.units),
                    _ => return Err(bad(expr)),
                };

                result
                    .map(Value::Quantity)
                    .map_err(|error| Diagnostic::error(error.message, expr.range()))
            }

            Expr::Call(call) => {
                let callee = call.callee().ok_or_else(|| bad(expr))?;
                let mut evaluated = Vec::new();
                for arg in call.args() {
                    let value = arg.value().ok_or_else(|| bad(expr))?;
                    evaluated.push(self.eval_expr(&value, scope, backend)?);
                }

                self.call(callee.text(), &evaluated, expr.range(), backend)
            }
        }
    }

    fn call<B: Backend>(
        &self,
        name: &str,
        args: &[Value<B::Body>],
        range: TextRange,
        backend: &mut B,
    ) -> Result<Value<B::Body>, Diagnostic> {
        // Pass one has already matched the arity of every known function, and
        // every unknown one leaves through the final arm below, so the indices
        // here are in range. `get` rather than `[]` all the same: this must
        // never be the thing that panics.
        let at = |index: usize| -> Result<&Value<B::Body>, Diagnostic> {
            args.get(index)
                .ok_or_else(|| Diagnostic::error(format!("`{name}` is missing an argument"), range))
        };

        let number = |index: usize| -> Result<Quantity, Diagnostic> {
            match at(index)? {
                Value::Quantity(quantity) => Ok(*quantity),
                Value::Body(_) => Err(Diagnostic::error("expected a number here", range)),
            }
        };

        let length = |index: usize| -> Result<f64, Diagnostic> {
            let value = number(index)?
                .coerce_to(Dimension::Length, self.units)
                .map(|quantity| quantity.value)
                .map_err(|error| Diagnostic::error(error.message, range))?;

            // A 400-digit literal saturates to infinity and arithmetic can
            // produce a NaN, and neither is a size anything can be built at.
            // The check lives here, at the last point before the backend, so
            // it holds for every backend rather than only the ones that check
            // for themselves.
            if !value.is_finite() {
                return Err(Diagnostic::error(
                    format!("`{name}` needs a finite length, found {value}"),
                    range,
                ));
            }

            Ok(value)
        };

        let body = |index: usize| -> Result<B::Body, Diagnostic> {
            match at(index)? {
                Value::Body(body) => Ok(body.clone()),
                Value::Quantity(_) => Err(Diagnostic::error("expected geometry here", range)),
            }
        };

        let scalar = |index: usize| -> Result<f64, Diagnostic> {
            let quantity = number(index)?;
            if quantity.dimension == Dimension::Scalar {
                Ok(quantity.value)
            } else {
                Err(Diagnostic::error(
                    format!("expected a scalar, found {}", quantity.dimension.name()),
                    range,
                ))
            }
        };

        let angle = |index: usize| -> Result<f64, Diagnostic> {
            let quantity = number(index)?;
            if quantity.dimension == Dimension::Angle {
                Ok(quantity.value)
            } else {
                Err(Diagnostic::error(
                    format!("expected an angle, found {}", quantity.dimension.name()),
                    range,
                ))
            }
        };

        let quantity =
            |value: f64, dimension: Dimension| Ok(Value::Quantity(Quantity { value, dimension }));

        match name {
            "cuboid" => backend
                .cuboid(length(0)?, length(1)?, length(2)?)
                .map(Value::Body)
                .map_err(|message| Diagnostic::error(message, range)),

            "cylinder" => backend
                .cylinder(length(0)?, length(1)?)
                .map(Value::Body)
                .map_err(|message| Diagnostic::error(message, range)),

            "cut" => backend
                .cut(&body(0)?, &body(1)?)
                .map(Value::Body)
                .map_err(|message| Diagnostic::error(message, range)),

            "sqrt" => quantity(scalar(0)?.sqrt(), Dimension::Scalar),
            "floor" => quantity(scalar(0)?.floor(), Dimension::Scalar),
            "ceil" => quantity(scalar(0)?.ceil(), Dimension::Scalar),
            "sin" => quantity(angle(0)?.sin(), Dimension::Scalar),
            "cos" => quantity(angle(0)?.cos(), Dimension::Scalar),
            "tan" => quantity(angle(0)?.tan(), Dimension::Scalar),

            "abs" => {
                let value = number(0)?;
                quantity(value.value.abs(), value.dimension)
            }

            "min" | "max" => {
                let a = number(0)?;
                let b = number(1)?;
                // Reuse the additive rules so scalar-and-length agree.
                let unified = a
                    .add(
                        Quantity {
                            value: 0.0,
                            dimension: b.dimension,
                        },
                        self.units,
                    )
                    .map_err(|error| Diagnostic::error(error.message, range))?;
                let b = b
                    .coerce_to(unified.dimension, self.units)
                    .map_err(|error| Diagnostic::error(error.message, range))?;
                let a = a
                    .coerce_to(unified.dimension, self.units)
                    .map_err(|error| Diagnostic::error(error.message, range))?;

                let value = if name == "min" {
                    a.value.min(b.value)
                } else {
                    a.value.max(b.value)
                };
                quantity(value, unified.dimension)
            }

            _ => Err(Diagnostic::error(
                format!("`{name}` is not a known function"),
                range,
            )),
        }
    }
}

fn bad(expr: &Expr) -> Diagnostic {
    Diagnostic::error("could not evaluate this expression", expr.range())
}

/// What a builtin's arguments must be.
///
/// Pass one asks this of every argument so that a dimension mistake is
/// reported alongside the document's other errors rather than one pass later.
/// The messages are the ones pass two would produce, so nothing a user reads
/// changes.
#[derive(Debug, Clone, Copy)]
enum Wants {
    Body,
    Length,
    Scalar,
    Angle,
    /// Any number, whatever its dimension.
    Number,
}

impl Wants {
    fn check(self, kind: Kind, units: LengthUnit) -> Result<(), String> {
        let dimension = match (self, kind) {
            (Wants::Body, Kind::Body) => return Ok(()),
            (Wants::Body, Kind::Quantity(_)) => return Err("expected geometry here".to_string()),
            (_, Kind::Body) => return Err("expected a number here".to_string()),
            (_, Kind::Quantity(dimension)) => dimension,
        };

        match self {
            Wants::Length => probe(dimension)
                .coerce_to(Dimension::Length, units)
                .map(|_| ())
                .map_err(|error| error.message),
            Wants::Scalar if dimension != Dimension::Scalar => {
                Err(format!("expected a scalar, found {}", dimension.name()))
            }
            Wants::Angle if dimension != Dimension::Angle => {
                Err(format!("expected an angle, found {}", dimension.name()))
            }
            // `Wants::Number` takes anything, and a satisfied `Scalar`/`Angle`
            // falls through to here.
            _ => Ok(()),
        }
    }
}

/// Where a builtin's result dimension comes from.
#[derive(Debug, Clone, Copy)]
enum ResultOf {
    Body,
    Scalar,
    /// The dimension of the first argument, unchanged.
    SameAsArgument,
    /// The additive unification of both arguments.
    UnifiedArguments,
}

/// What a builtin accepts and produces.
struct Signature {
    params: &'static [&'static str],
    wants: Wants,
    result: ResultOf,
}

fn signature_of(name: &str) -> Option<Signature> {
    Some(match name {
        "cuboid" => Signature {
            params: &["dx", "dy", "dz"],
            wants: Wants::Length,
            result: ResultOf::Body,
        },
        "cylinder" => Signature {
            params: &["radius", "height"],
            wants: Wants::Length,
            result: ResultOf::Body,
        },
        "cut" => Signature {
            params: &["target", "tool"],
            wants: Wants::Body,
            result: ResultOf::Body,
        },
        "sqrt" | "floor" | "ceil" => Signature {
            params: &["x"],
            wants: Wants::Scalar,
            result: ResultOf::Scalar,
        },
        "sin" | "cos" | "tan" => Signature {
            params: &["x"],
            wants: Wants::Angle,
            result: ResultOf::Scalar,
        },
        "abs" => Signature {
            params: &["x"],
            wants: Wants::Number,
            result: ResultOf::SameAsArgument,
        },
        "min" | "max" => Signature {
            params: &["x", "y"],
            wants: Wants::Number,
            result: ResultOf::UnifiedArguments,
        },
        _ => return None,
    })
}

/// STEP or STL, decided by extension.
pub fn extension_format(path: &str) -> Option<&'static str> {
    let lowered = path.to_ascii_lowercase();
    if lowered.ends_with(".step") || lowered.ends_with(".stp") {
        Some("step")
    } else if lowered.ends_with(".stl") {
        Some("stl")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::RecordingBackend;
    use std::path::Path;

    fn run(source: &str) -> Result<RecordingBackend, Vec<Diagnostic>> {
        let mut backend = RecordingBackend::default();
        evaluate(source, Path::new("/tmp"), &mut backend)?;
        Ok(backend)
    }

    fn errors(source: &str) -> Vec<String> {
        match run(source) {
            Ok(_) => panic!("expected failure"),
            Err(diagnostics) => diagnostics.into_iter().map(|d| d.message).collect(),
        }
    }

    #[test]
    fn evaluates_a_document_into_kernel_calls() {
        let backend = run("units mm\n\
             param w = 60\n\
             body plate = cuboid(w, 40, 12)\n\
             body hole = cylinder(radius = 5, height = 12)\n\
             body part = cut(plate, hole)\n\
             export \"part.step\" from part\n")
        .unwrap();

        assert_eq!(
            backend.calls,
            vec![
                "cuboid(60, 40, 12)".to_string(),
                "cylinder(5, 12)".to_string(),
                "cut(#0, #1)".to_string(),
                "export(#2, /tmp/part.step)".to_string(),
            ]
        );
    }

    #[test]
    fn document_units_scale_bare_numbers() {
        let backend = run("units cm\nbody b = cuboid(1, 2, 3)\n").unwrap();
        assert_eq!(backend.calls, vec!["cuboid(10, 20, 30)".to_string()]);
    }

    #[test]
    fn reports_every_error_not_just_the_first() {
        let messages = errors("param a = nope\nparam b = alsonope\n");
        assert_eq!(messages.len(), 2, "{messages:?}");
    }

    #[test]
    fn rejects_a_forward_reference() {
        let messages = errors("param a = b\nparam b = 1\n");
        assert!(messages[0].contains("`b`"), "{messages:?}");
    }

    #[test]
    fn rejects_a_duplicate_declaration() {
        let messages = errors("param a = 1\nparam a = 2\n");
        assert!(messages[0].contains("already declared"), "{messages:?}");
    }

    #[test]
    fn rejects_binding_geometry_to_a_param() {
        let messages = errors("param a = cuboid(1, 2, 3)\n");
        assert!(messages[0].contains("param"), "{messages:?}");
    }

    #[test]
    fn rejects_binding_a_number_to_a_body() {
        let messages = errors("body a = 5\n");
        assert!(messages[0].contains("body"), "{messages:?}");
    }

    #[test]
    fn rejects_a_dimension_mismatch() {
        let messages = errors("param a = 1mm + 45deg\n");
        assert!(messages[0].contains("add"), "{messages:?}");
    }

    #[test]
    fn rejects_wrong_arity() {
        let messages = errors("body b = cuboid(1, 2)\n");
        assert!(messages[0].contains("expects"), "{messages:?}");
    }

    #[test]
    fn rejects_an_unknown_function() {
        let messages = errors("body b = sphere(1)\n");
        assert!(messages[0].contains("sphere"), "{messages:?}");
    }

    #[test]
    fn evaluates_builtin_maths() {
        let backend = run("param s = sqrt(16)\nbody b = cuboid(s, 1, 1)\n").unwrap();
        assert_eq!(backend.calls, vec!["cuboid(4, 1, 1)".to_string()]);
    }

    #[test]
    fn export_without_from_requires_exactly_one_body() {
        let messages =
            errors("body a = cuboid(1,1,1)\nbody b = cuboid(2,2,2)\nexport \"x.step\"\n");
        assert!(messages[0].contains("which body"), "{messages:?}");
    }

    #[test]
    fn rejects_an_unknown_export_extension() {
        let messages = errors("body a = cuboid(1,1,1)\nexport \"x.obj\" from a\n");
        assert!(messages[0].contains("obj"), "{messages:?}");
    }

    #[test]
    fn a_document_with_errors_produces_no_calls() {
        let mut backend = RecordingBackend::default();
        let result = evaluate(
            "body a = cuboid(1,1,1)\nexport \"x.step\" from a\nparam bad = nope\n",
            Path::new("/tmp"),
            &mut backend,
        );

        assert!(result.is_err());
        assert!(
            backend.calls.is_empty(),
            "geometry ran anyway: {:?}",
            backend.calls
        );
    }

    // Beyond the brief.

    #[test]
    fn an_unparseable_document_is_not_an_empty_one() {
        // `Document::statements` skips `Error` nodes, so a document nothing
        // could be made of has no statements and would evaluate cleanly if the
        // parser's own errors were not folded in first.
        for source in ["!!!\n", "param x = 1\n)\nbody b = 2\n", "@@@ &&& ???\n"] {
            let messages = errors(source);
            assert!(!messages.is_empty(), "{source:?} reported nothing");
        }
    }

    #[test]
    fn rejects_a_length_that_is_not_finite() {
        // `f64::from_str` saturates a 400-digit literal to infinity rather than
        // failing, and `RecordingBackend` accepts whatever it is handed — so
        // without a check here a lang-only test would pass while the kernel,
        // whose `check_extent` rejects non-finite extents, would fail.
        let huge = "1".repeat(400);
        let messages = errors(&format!("body b = cuboid({huge}, 1, 1)\n"));
        assert!(messages[0].contains("finite"), "{messages:?}");

        // No literal can be a NaN, but arithmetic can produce one.
        let messages = errors("param n = sqrt(-1)\nbody b = cuboid(n, 1, 1)\n");
        assert!(messages[0].contains("finite"), "{messages:?}");
    }

    #[test]
    fn a_failure_after_an_export_still_writes_nothing() {
        // Pass one cannot know a value is not finite, so this only fails in
        // pass two — after the `export` statement has already been read. No
        // file may be written all the same: an artefact that looks fresh but
        // predates the failure is worse than none.
        let mut backend = RecordingBackend::default();
        let huge = "1".repeat(400);
        let result = evaluate(
            &format!(
                "body a = cuboid(1, 1, 1)\nexport \"a.step\" from a\nbody b = cuboid({huge}, 1, 1)\n"
            ),
            Path::new("/tmp"),
            &mut backend,
        );

        assert!(result.is_err());
        assert!(
            !backend.calls.iter().any(|call| call.starts_with("export")),
            "exported despite a failure: {:?}",
            backend.calls
        );
    }

    #[test]
    fn division_by_zero_is_left_to_the_second_pass() {
        // Pass one probes the dimension rules with 1.0 on both sides, so it
        // cannot see this — deliberately, because a placeholder denominator
        // must never invent an error. The second pass still catches it.
        let messages = errors("param x = 1 / 0\n");
        assert!(messages[0].contains("division by zero"), "{messages:?}");
    }

    #[test]
    fn pass_one_agrees_with_pass_two_about_result_dimensions() {
        // `abs` returns the dimension it was given and `min`/`max` unify their
        // arguments. If pass one claimed a scalar for either, these documents
        // would be rejected for errors the evaluated document does not have.
        let backend =
            run("param a = abs(-45deg)\nparam b = a + 45deg\nbody c = cuboid(1, 1, 1)\n").unwrap();
        assert_eq!(backend.calls, vec!["cuboid(1, 1, 1)".to_string()]);

        let backend = run("param m = min(10mm, 2)\nbody b = cuboid(m, 1, 1)\n").unwrap();
        assert_eq!(backend.calls, vec!["cuboid(2, 1, 1)".to_string()]);
    }

    #[test]
    fn pass_one_reports_argument_dimensions_rather_than_leaving_them_to_pass_two() {
        // Both errors must appear together: fixing one, rerunning and finding
        // the next is exactly what the first pass exists to prevent.
        let messages = errors("body b = cuboid(45deg, 1, 1)\nparam x = sin(1)\n");
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(messages[0].contains("expected length"), "{messages:?}");
        assert!(messages[1].contains("expected an angle"), "{messages:?}");
    }

    #[test]
    fn rejects_a_named_argument_in_the_wrong_position() {
        // An argument is bound by position; the name only labels it. Accepting
        // a swap would build a 5mm-radius cylinder from a document that says
        // 12 — wrong geometry with no diagnostic, which is the worst outcome
        // available to a CAD tool.
        let messages = errors("body b = cylinder(height = 5, radius = 12)\n");
        assert!(messages[0].contains("`radius`"), "{messages:?}");

        // Naming the same parameter twice leaves the other unmentioned.
        let messages = errors("body b = cylinder(radius = 5, radius = 12)\n");
        assert!(messages[0].contains("`height`"), "{messages:?}");

        // A name that is no parameter at all keeps its own message.
        let messages = errors("body b = cylinder(radius = 5, nope = 12)\n");
        assert!(messages[0].contains("has no parameter"), "{messages:?}");

        // Naming every argument in order stays legal.
        let backend = run("body b = cylinder(radius = 5, height = 12)\n").unwrap();
        assert_eq!(backend.calls, vec!["cylinder(5, 12)".to_string()]);
    }

    #[test]
    fn no_document_makes_the_evaluator_panic() {
        // The parser recovers from anything, so this layer is routinely handed
        // half-built trees. Reporting is its whole job; a panic would take the
        // caller down instead of telling the user what is wrong.
        let sources = [
            "",
            "\n\n\n",
            "param",
            "param =",
            "param x =",
            "param x = 1 +",
            "param x = (",
            "body",
            "body b = f(",
            "body b = f(,)",
            "body b = f(= 1)",
            "body b = f(1 = 2)",
            "body b = f(x =)",
            "units",
            "units furlong",
            "units mm\nunits cm",
            "param x = 1\nunits mm",
            "export",
            "export from part",
            "export \"a.step\" from",
            "export \"a.step\"",
            "export \"\" from a",
            "!!! param x = 1",
            "param x = 1\n)\nbody b = 2",
            "param x = 1 / 0",
            "param x = -(-(-1))",
            "body b = -cuboid(1, 1, 1)",
            "body b = cut(1, 2)",
            "param x = cuboid(1, 1, 1) + 1",
            "param x = abs(cuboid(1, 1, 1))",
            "body b = cylinder(radius = 1, nope = 2)",
            "body b = cylinder(1)",
            "param x = 1e5",
            "param x = 5furlong",
            "param x = 1.2.3",
            "export \"a.step\" from missing",
            "param a = 1\nbody a = cuboid(1, 1, 1)",
            "body a = cuboid(1, 1, 1)\nexport \"a\" from a",
            "param a = a",
            "param x = min(1mm, 45deg)",
            "param x = ٣mm",
        ];

        for source in sources {
            let mut backend = RecordingBackend::default();
            let _ = evaluate(source, Path::new("/tmp"), &mut backend);
        }
    }

    /// A known gap, pinned so that closing it is a deliberate change with a
    /// failing test rather than an accident.
    #[test]
    fn an_export_path_is_not_confined_to_the_base_directory() {
        // `Path::join` walks out of `base_dir` for a relative path and discards
        // it entirely for an absolute one, so a document chooses where it
        // writes. Whether that is allowed is a policy the CLI owns, not the
        // language — but the language should not be read as enforcing it.
        let backend = run("body a = cuboid(1, 1, 1)\nexport \"../out.step\" from a\n").unwrap();
        assert_eq!(backend.calls[1], "export(#0, /tmp/../out.step)");

        let backend = run("body a = cuboid(1, 1, 1)\nexport \"/other/out.step\" from a\n").unwrap();
        assert_eq!(backend.calls[1], "export(#0, /other/out.step)");
    }
}
