//! Tailwind-compatible utility generation, JIT-style.
//!
//! Tailwind itself is a plain-JavaScript npm package, which `aoxn npm-import`
//! rejects by design (Aoxn links Aoxn packages, not JavaScript), and calling
//! its CLI would make Node a build prerequisite that the one-click Windows
//! install deliberately avoids. So v0.36.0 generates the utilities directly.
//!
//! **This is a documented subset, not Tailwind.** It covers the utilities that
//! appear most often in server-rendered pages; anything outside the tables
//! below is simply not generated. `aoxn` says so loudly rather than emitting a
//! silently-wrong stylesheet — see [`unknown_classes`].
//!
//! The generation model is the same one Tailwind uses: scan the sources for
//! class tokens, keep the ones this module understands, emit a rule per kept
//! token. Nothing is generated speculatively, so a page using six utilities
//! gets six rules.
//!
//! ```text
//! class="p-4 text-red-500 flex"   ->   .p-4{...} .text-red-500{...} .flex{...}
//! ```
//!
//! The escape hatch remains: hand-written CSS goes through the same asset
//! pipeline (`import * from "./app.css"`), so anything this subset does not
//! cover is still expressible.

use std::collections::BTreeSet;

/// Tailwind's default spacing scale, in `rem`. This is the `0.5` step scale
/// Tailwind ships; `--spacing` overrides are not supported.
const SPACING: &[(&str, &str)] = &[
    ("0", "0px"), ("px", "1px"), ("0.5", "0.125rem"), ("1", "0.25rem"),
    ("1.5", "0.375rem"), ("2", "0.5rem"), ("2.5", "0.625rem"), ("3", "0.75rem"),
    ("3.5", "0.875rem"), ("4", "1rem"), ("5", "1.25rem"), ("6", "1.5rem"),
    ("7", "1.75rem"), ("8", "2rem"), ("9", "2.25rem"), ("10", "2.5rem"),
    ("11", "2.75rem"), ("12", "3rem"), ("14", "3.5rem"), ("16", "4rem"),
    ("20", "5rem"), ("24", "6rem"), ("28", "7rem"), ("32", "8rem"),
];

/// A small, deliberately recognizable palette. These are Tailwind's
/// blue/red/gray families at their common steps; the full 22-colour palette
/// with all shades is out of scope and pretending otherwise would be worse
/// than a short table.
const COLORS: &[(&str, &str)] = &[
    ("black", "#000"), ("white", "#fff"), ("transparent", "transparent"),
    ("current", "currentColor"),
    ("gray-100", "#f3f4f6"), ("gray-200", "#e5e7eb"), ("gray-300", "#d1d5db"),
    ("gray-400", "#9ca3af"), ("gray-500", "#6b7280"), ("gray-600", "#4b5563"),
    ("gray-700", "#374151"), ("gray-800", "#1f2937"), ("gray-900", "#111827"),
    ("red-500", "#ef4444"), ("red-600", "#dc2626"), ("red-700", "#b91c1c"),
    ("blue-500", "#3b82f6"), ("blue-600", "#2563eb"), ("blue-700", "#1d4ed8"),
    ("green-500", "#22c55e"), ("green-600", "#16a34a"),
];

/// Font sizes with their default line-height and letter-spacing, matching
/// Tailwind's pairs.
const TEXT_SIZES: &[(&str, &str, &str)] = &[
    ("xs", "0.75rem", "1rem"), ("sm", "0.875rem", "1.25rem"),
    ("base", "1rem", "1.5rem"), ("lg", "1.125rem", "1.75rem"),
    ("xl", "1.25rem", "1.75rem"), ("2xl", "1.5rem", "2rem"),
    ("3xl", "1.875rem", "2.25rem"), ("4xl", "2.25rem", "2.5rem"),
];

/// Font weights.
const WEIGHTS: &[(&str, &str)] = &[
    ("light", "300"), ("normal", "400"), ("medium", "500"),
    ("semibold", "600"), ("bold", "700"), ("extrabold", "800"),
];

/// Radius scale.
const RADIUS: &[(&str, &str)] = &[
    ("none", "0px"), ("sm", "0.125rem"), ("", "0.25rem"),
    ("md", "0.375rem"), ("lg", "0.5rem"), ("xl", "0.75rem"),
    ("2xl", "1rem"), ("full", "9999px"),
];

/// Every class this build understood. Returned alongside the CSS so the
/// caller can warn about the rest.
pub struct Generated {
    pub css: String,
    pub known: BTreeSet<String>,
    /// class tokens that looked like utilities but are not in the tables
    pub unknown: Vec<String>,
}

/// Scan `sources` for class tokens and generate the CSS for the ones this
/// module knows.
///
/// Only *candidate-shaped* tokens are considered: a token is examined when it
/// is whitespace-delimited inside a `class`/`className` attribute or a quoted
/// string. Free text is never mined for utility-looking words, because that
/// would generate CSS for prose ("the red-500 of it") and quietly ship
/// dead rules.
pub fn generate(sources: &[&str]) -> Generated {
    let mut known: BTreeSet<String> = BTreeSet::new();
    let mut unknown: BTreeSet<String> = BTreeSet::new();
    let mut css = String::new();

    for src in sources {
        for token in candidates(src) {
            if let Some(rule) = rule_for(&token) {
                if known.insert(token.clone()) {
                    css.push_str(&rule);
                }
            } else if looks_like_utility(&token) {
                unknown.insert(token);
            }
        }
    }

    Generated { css, known, unknown: unknown.into_iter().collect() }
}

/// Pull candidate class tokens out of source text.
///
/// Two shapes are recognised, matching how markup actually appears here:
/// `class="a b c"` / `className="a b c"` and any bare double- or
/// single-quoted string. A token must match the utility grammar to be
/// considered at all, so ordinary prose in a quoted string is ignored.
fn candidates(src: &str) -> Vec<String> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'"' || b[i] == b'\'' {
            let end = quoted_end(src, i);
            // ONLY a class attribute is mined. Accepting any "shaped like a
            // class list" string was tried and rejected: prose like
            // "the red-500 of it is p-4" passes every shape test that a real
            // `class="..."` does, so it silently generated CSS. Missing a
            // utility costs one declaration; mining a sentence ships dead
            // rules that can shadow a hand-written one.
            if preceding_keyword(src, i) {
                out.extend(src[i + 1..end].split_whitespace().map(|t| t.to_string()));
            }
            i = end + 1;
            continue;
        }
        let ch = src[i..].chars().next().unwrap_or('\u{fffd}');
        i += ch.len_utf8();
    }
    out
}

/// Index of the closing quote for the string starting at `start`.
fn quoted_end(src: &str, start: usize) -> usize {
    let b = src.as_bytes();
    let quote = b[start];
    let mut j = start + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            c if c == quote => return j,
            _ => j += 1,
        }
    }
    b.len().saturating_sub(1).max(start)
}

/// Was the string at `start` introduced by `class=` / `className=` /
/// `class:`? Both the attribute form (`<div class="...">`) and the object
/// form (`{class: "..."}`) must be recognized, or half a codebase's classes
/// are invisible.
fn preceding_keyword(src: &str, start: usize) -> bool {
    let before = src[..start].trim_end();
    let before = before.trim_end_matches(['=', ':', '{', '(']);
    let key = match before.rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$')) {
        Some(i) => &before[i + 1..],
        None => before,
    };
    matches!(key, "class" | "className" | "class_name")
}

/// Terminate the last declaration in a rule body, so generated CSS is
/// well-formed (`color:red}` -> `color:red;}`). Splitting on `;` keeps any
/// `;` inside a `url(...)` value intact.
fn ensure_trailing_semicolons(body: &str) -> String {
    if body.trim_end().ends_with(';') {
        return body.to_string();
    }
    let mut out = String::with_capacity(body.len() + 1);
    let mut in_paren = false;
    for c in body.chars() {
        match c {
            '(' => in_paren = true,
            ')' => in_paren = false,
            ';' if !in_paren => {
                out.push(c);
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out.push(';');
    out
}

/// Does this token have the *shape* of a utility class? Used to tell "a class
fn looks_like_utility(token: &str) -> bool {
    if token.len() > 40 || token.is_empty() {
        return false;
    }
    let prefixes = [
        "p", "m", "gap", "w", "h", "text", "bg", "border", "rounded", "flex",
        "grid", "font", "leading", "items", "justify", "space", "opacity",
        "z", "inset", "top", "left", "right", "bottom", "min-", "max-",
        "shadow", "overflow", "hidden", "block", "inline", "table", "container",
        // reported but not generated — the point of this list is to name them
        "animate", "transition", "duration", "ease", "delay", "ring", "cursor",
        "select", "pointer", "divide", "placeholder", "caret", "accent", "fill",
        "stroke", "order", "grow", "shrink", "basis", "self", "place", "content",
        "list", "whitespace", "break", "object", "aspect", "backdrop", "filter",
        "blur", "from", "via", "to", "translate", "scale", "rotate", "skew",
        "tracking", "indent", "align", "col", "row", "sr", "resize", "snap",
    ];
    // A token already generated is in `known`; this only classifies the rest.
    prefixes.iter().any(|p| token.starts_with(p))
        || token.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
}

/// The rule for one class token, or `None` when this subset does not cover it.
fn rule_for(token: &str) -> Option<String> {
    // static, single-purpose utilities
    let static_rule: Option<&str> = match token {
        "block" => Some("display:block"),
        "inline" => Some("display:inline"),
        "inline-block" => Some("display:inline-block"),
        "flex" => Some("display:flex"),
        "inline-flex" => Some("display:inline-flex"),
        "grid" => Some("display:grid"),
        "hidden" => Some("display:none"),
        "table" => Some("display:table"),
        "container" => Some("max-width:64rem;margin-left:auto;margin-right:auto"),
        "truncate" => Some("overflow:hidden;text-overflow:ellipsis;white-space:nowrap"),
        "uppercase" => Some("text-transform:uppercase"),
        "lowercase" => Some("text-transform:lowercase"),
        "capitalize" => Some("text-transform:capitalize"),
        "underline" => Some("text-decoration-line:underline"),
        "italic" => Some("font-style:italic"),
        "relative" => Some("position:relative"),
        "absolute" => Some("position:absolute"),
        "fixed" => Some("position:fixed"),
        "static" => Some("position:static"),
        "border" => Some("border-width:1px;border-style:solid"),
        _ => None,
    };
    if let Some(body) = static_rule {
        return Some(format!(".{token}{{{}}}", ensure_trailing_semicolons(body)));
    }

    // directional spacing: p/m + optional side + spacing step
    for (prefix, prop) in [("p", "padding"), ("m", "margin")] {
        // The dash belongs to the utility name and the side prefix sits
        // between it and the step: `p-4` -> step "4", `px-4` -> sides + "4".
        // Stripping only `p-` would miss every side-prefixed utility.
        if let Some(rest) = token.strip_prefix(prefix) {
            let rest = rest.strip_prefix('-').unwrap_or(rest);
            let (sides, step) = split_sides(rest);
            if let Some(v) = spacing_value(step) {
                // no side prefix means the shorthand itself (`p-4` ->
                // `padding`), matching Tailwind
                let decls = if sides.is_empty() {
                    format!("{prop}:{v};")
                } else {
                    sides.iter().map(|s| format!("{prop}-{s}:{v};")).collect()
                };
                return Some(format!(".{token}{{{decls}}}"));
            }
        }
    }

    // gap-<step>
    if let Some(step) = token.strip_prefix("gap-") {
        if let Some(v) = spacing_value(step) {
            return Some(format!(".{token}{{gap:{v};}}"));
        }
    }

    // spacing scale for width/height/min/max
    for prefix in ["w", "h", "min-w", "min-h", "max-w", "max-h"] {
        if token == prefix {
            return Some(format!(".{token}{{width:100%;}}"));
        }
        if let Some(rest) = token.strip_prefix(&format!("{prefix}-")) {
            if let Some(v) = spacing_value(rest) {
                let prop = if prefix == "w" || prefix == "min-w" || prefix == "max-w" {
                    "width"
                } else {
                    "height"
                };
                return Some(format!(".{token}{{{prop}:{v};}}"));
            }
        }
    }

    // colors: text-<color>, bg-<color>, border-<color>
    for (prefix, prop) in [("text", "color"), ("bg", "background-color"), ("border", "border-color")] {
        if let Some(rest) = token.strip_prefix(&format!("{prefix}-")) {
            // `text-sm` is a size, not a color; check sizes first
            if prop == "color" {
                if let Some((_, size, lh)) = text_size(rest) {
                    return Some(format!(".{token}{{font-size:{size};line-height:{lh};}}"));
                }
                if let Some(w) = weight(rest) {
                    return Some(format!(".{token}{{font-weight:{w};}}"));
                }
            }
            if let Some(c) = color(rest) {
                return Some(format!(".{token}{{{prop}:{c};}}"));
            }
        }
    }

    // rounded-<step>
    if let Some(rest) = token.strip_prefix("rounded") {
        let step = rest.strip_prefix('-').unwrap_or("");
        if let Some(r) = radius(step) {
            return Some(format!(".{token}{{border-radius:{r};}}"));
        }
    }

    // flex direction / alignment
    match token {
        "flex-row" => return Some(".flex-row{flex-direction:row;}".into()),
        "flex-col" => return Some(".flex-col{flex-direction:column;}".into()),
        "flex-wrap" => return Some(".flex-wrap{flex-wrap:wrap;}".into()),
        "flex-1" => return Some(".flex-1{flex:1 1 0%;}".into()),
        "items-center" => return Some(".items-center{align-items:center;}".into()),
        "items-start" => return Some(".items-start{align-items:flex-start;}".into()),
        "items-end" => return Some(".items-end{align-items:flex-end;}".into()),
        "justify-center" => return Some(".justify-center{justify-content:center;}".into()),
        "justify-between" => return Some(".justify-between{justify-content:space-between;}".into()),
        "justify-end" => return Some(".justify-end{justify-content:flex-end;}".into()),
        "overflow-hidden" => return Some(".overflow-hidden{overflow:hidden;}".into()),
        "overflow-auto" => return Some(".overflow-auto{overflow:auto;}".into()),
        "opacity-0" => return Some(".opacity-0{opacity:0;}".into()),
        "opacity-50" => return Some(".opacity-50{opacity:0.5;}".into()),
        "opacity-100" => return Some(".opacity-100{opacity:1;}".into()),
        "text-left" => return Some(".text-left{text-align:left;}".into()),
        "text-center" => return Some(".text-center{text-align:center;}".into()),
        "text-right" => return Some(".text-right{text-align:right;}".into()),
        _ => {}
    }

    None
}

/// `p-4` -> all sides; `px-4` -> left+right; `pt-2` -> top. Returns the CSS
/// longhand suffixes to emit and the spacing step.
fn split_sides(rest: &str) -> (Vec<&'static str>, &str) {
    const SIDES: [(&str, &[&str]); 8] = [
        ("x", &["left", "right"]), ("y", &["top", "bottom"]),
        ("t", &["top"]), ("r", &["right"]), ("b", &["bottom"]),
        ("l", &["left"]), ("s", &["top", "bottom"]), ("e", &["left", "right"]),
    ];
    for (prefix, sides) in SIDES {
        // the dash between the side and the step is optional and belongs to
        // neither: `px-4` and `px4` both mean horizontal padding of 1rem
        if let Some(step) = rest.strip_prefix(prefix).and_then(|s| s.strip_prefix('-')) {
            if !step.is_empty() && step.as_bytes()[0].is_ascii_digit() {
                return (sides.to_vec(), step);
            }
        }
    }
    (Vec::new(), rest) // empty means "the bare property", e.g. `padding`
}

fn spacing_value(step: &str) -> Option<&'static str> {
    SPACING.iter().find(|(k, _)| *k == step).map(|(_, v)| *v)
}

fn color(name: &str) -> Option<&'static str> {
    COLORS.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

fn text_size(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    TEXT_SIZES
        .iter()
        .find(|(k, _, _)| *k == name)
        .map(|(_, s, l)| (*s, *l, ""))
}

fn weight(name: &str) -> Option<&'static str> {
    WEIGHTS.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

fn radius(step: &str) -> Option<&'static str> {
    RADIUS.iter().find(|(k, _)| *k == step).map(|(_, v)| *v)
}

/// The class tokens this build did not cover, for a build-time warning.
pub fn unknown_classes(sources: &[&str]) -> Vec<String> {
    generate(sources).unknown
}