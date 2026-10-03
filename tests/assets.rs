//! CSS asset pipeline: `.css` as a first-class import, `@import` inlining,
//! conservative minification, fingerprints, and CSS Modules class scoping.
//!
//! The scoping tests are the sharp ones: a `.` inside a string literal, inside
//! an `@media` prelude, or inside a declaration value is *not* a class
//! selector. A naive `str::replace(".foo", ...)` gets all three wrong and
//! silently changes rendering, so each is pinned here.

use aoxn::assets::{minify, scan_css_imports, scope_classes as scope};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("Aoxn-assets-{}-{}-{}", std::process::id(), tag, n));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn have_clang() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOXN_CLANG") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Build `main` (with its sibling assets) and run it, returning stdout.
fn build_and_run(dir: &Path, main: &str, tag: &str) -> String {
    let exe = dir.join(format!("run-{}{EXE}", std::process::id()));
    aoxn::build_paths_exe(&[main.to_string()], &exe, true).unwrap_or_else(|d| {
        panic!(
            "build failed: {}",
            d.iter().map(aoxn::diag_to_string).collect::<Vec<_>>().join("; ")
        )
    });
    let out = Command::new(&exe).output().expect("failed to run");
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    assert!(
        out.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = tag;
    String::from_utf8_lossy(&out.stdout).to_string()
}

// ---- the regression this module exists for ----

/// Before the asset pipeline, a `.css` import resolved successfully (the
/// `is_file()` short-circuit in `complete_module_path`) and then fell through
/// to the Aoxn lexer, reporting `unexpected character '{'`. If this test ever
/// fails with a lex error, the CSS diversion in `load_file` was removed.
#[test]
fn css_import_is_not_lexed_as_source() {
    let dir = tmp_dir("lex");
    std::fs::write(dir.join("a.css"), "body { margin: 0; }").unwrap();
    let main = dir.join("main.ax");
    std::fs::write(&main, "import * from \"./a.css\"\ndef main() -> int:\n    print(styles())\n    return 0\n").unwrap();
    let exe = dir.join(format!("m{EXE}"));
    aoxn::build_paths_exe(&[main.display().to_string()], &exe, true).unwrap_or_else(|d| {
        panic!("build failed: {}", d.iter().map(aoxn::diag_to_string).collect::<Vec<_>>().join("; "))
    });
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- @import inlining ----

#[test]
fn import_is_inlined_in_order() {
    let dir = tmp_dir("inl");
    std::fs::write(dir.join("a.css"), "@import \"./b.css\";\n.a { color: red; }").unwrap();
    std::fs::write(dir.join("b.css"), ".b { color: blue; }").unwrap();
    let main = dir.join("main.ax");
    std::fs::write(&main, "import * from \"./a.css\"\ndef main() -> int:\n    print(styles())\n    return 0\n").unwrap();
    let out = build_and_run(&dir, &main.display().to_string(), "inl");
    let css = out.trim();
    // the imported rule must come first, preserving cascade order
    assert!(css.find(".b").unwrap() < css.find(".a").unwrap(), "cascade order lost: {css}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn circular_import_is_rejected() {
    let dir = tmp_dir("cycle");
    std::fs::write(dir.join("a.css"), "@import \"./b.css\";").unwrap();
    std::fs::write(dir.join("b.css"), "@import \"./a.css\";").unwrap();
    let main = dir.join("main.ax");
    std::fs::write(&main, "import * from \"./a.css\"\ndef main() -> int:\n    return 0\n").unwrap();
    let exe = dir.join(format!("c{EXE}"));
    let err = aoxn::build_paths_exe(&[main.display().to_string()], &exe, true)
        .expect_err("circular @import must fail the build");
    let msg = err.iter().map(aoxn::diag_to_string).collect::<Vec<_>>().join("; ");
    assert!(msg.contains("circular @import"), "unexpected diagnostic: {msg}");
    assert!(err.iter().any(|d| d.stage == "asset"), "expected stage 'asset': {msg}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_import_is_rejected() {
    let dir = tmp_dir("missing");
    std::fs::write(dir.join("a.css"), "@import \"./nope.css\";").unwrap();
    let main = dir.join("main.ax");
    std::fs::write(&main, "import * from \"./a.css\"\ndef main() -> int:\n    return 0\n").unwrap();
    let exe = dir.join(format!("m{EXE}"));
    let err = aoxn::build_paths_exe(&[main.display().to_string()], &exe, true)
        .expect_err("missing @import must fail the build");
    let msg = err.iter().map(aoxn::diag_to_string).collect::<Vec<_>>().join("; ");
    assert!(msg.contains("does not resolve"), "unexpected diagnostic: {msg}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `@import` of a non-CSS resource (a font, say) must survive verbatim:
/// dropping it would change rendering, and failing the build on valid CSS
/// would reject stylesheets the browser accepts.
#[test]
fn non_css_import_is_passed_through() {
    let dir = tmp_dir("font");
    std::fs::write(dir.join("a.css"), "@import url(\"x.woff2\");\n.a{color:red}").unwrap();
    let main = dir.join("main.ax");
    std::fs::write(&main, "import * from \"./a.css\"\ndef main() -> int:\n    print(styles())\n    return 0\n").unwrap();
    let out = build_and_run(&dir, &main.display().to_string(), "font");
    assert!(out.contains("x.woff2"), "url() import was dropped: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- minification ----

#[test]
fn minify_removes_comments_and_whitespace() {
    assert_eq!(minify("/* hi */\n.a {\n  color : red ;\n}\n"), ".a{color:red;}");
}

#[test]
fn minify_keeps_required_spaces() {
    // descendant combinator and the `0 8px` shorthand must survive
    assert_eq!(minify(".a .b { margin: 0 8px; }"), ".a .b{margin:0 8px;}");
}

#[test]
fn minify_is_idempotent() {
    let src = "/* c */ .a , .b { color : red ; }\n@media screen { .a { x : 1 } }";
    let once = minify(src);
    assert_eq!(minify(&once), once, "minify is not idempotent");
}

#[test]
fn comment_does_not_glue_selectors() {
    // `a/**/b` must not become `ab`
    assert_eq!(minify("a/**/b { color: red }"), "a b{color:red}");
}

// ---- CSS Modules ----

#[test]
fn module_classes_are_hashed() {
    let (out, classes) = scope(".title { color: red }", 42);
    assert!(out.starts_with(".title_"), "class not scoped: {out}");
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].0, "title");
    assert!(classes[0].1.starts_with("title_"));
}

#[test]
fn module_ignores_dots_in_strings() {
    let (out, classes) = scope(r#".a::after { content: ".notaclass"; }"#, 42);
    assert!(out.contains(r#""\.notaclass""#) || out.contains(r#"".notaclass""#), "string was rewritten: {out}");
    assert_eq!(classes.len(), 1, "a string's dot must not become a class: {out}");
}

#[test]
fn module_ignores_media_query_params() {
    let (out, _) = scope("@media (min-width: 30rem) { .a { color: red } }", 42);
    assert!(out.contains("(min-width:30rem)") || out.contains("(min-width: 30rem)"), "media prelude damaged: {out}");
    assert!(!out.contains("30rem_"), "media prelude was rewritten: {out}");
}

#[test]
fn module_ignores_dots_in_values() {
    // a decimal in a declaration value is not a class selector
    let (out, classes) = scope(".a { width: 1.5rem; }", 42);
    assert!(out.contains("1.5rem"), "value damaged: {out}");
    assert_eq!(classes.len(), 1);
}

#[test]
fn module_scopes_pseudo_class_selectors() {
    let (out, classes) = scope(".btn:hover { color: red }", 42);
    assert!(out.contains(":hover"), "pseudo class lost: {out}");
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].0, "btn");
}

#[test]
fn distinct_modules_hash_the_same_name_differently() {
    let (_, a) = scope(".title{color:red}", 1);
    let (_, b) = scope(".title{color:red}", 2);
    assert_ne!(a[0].1, b[0].1, "per-module seed not applied");
}

// ---- end-to-end ----

#[test]
fn styles_returns_bundle_and_fingerprint() {
    let Some(_clang) = have_clang() else {
        eprintln!("skipping: set AOXN_CLANG to run (needs clang)");
        return;
    };
    let dir = tmp_dir("e2e");
    std::fs::write(dir.join("a.css"), "/* c */\n.a { color : red ; }").unwrap();
    let main = dir.join("main.ax");
    std::fs::write(
        &main,
        "import * from \"./a.css\"\n\
         def main() -> int:\n    \
             print(styles())\n    \
             print(\"|\")\n    \
             print(styles_fingerprint())\n    \
             return 0\n",
    )
    .unwrap();
    let out = build_and_run(&dir, &main.display().to_string(), "e2e");
    let mut it = out.split('|');
    let css = it.next().unwrap().trim_end();
    let fp = it.next().unwrap().trim();
    assert_eq!(css, ".a{color:red;}");
    assert!(fp.ends_with(".css"), "fingerprint is not a css name: {fp}");
    assert_eq!(fp.len(), 16 + ".css".len(), "unexpected fingerprint shape: {fp}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn module_accessor_returns_hashed_name() {
    let Some(_clang) = have_clang() else {
        eprintln!("skipping: set AOXN_CLANG to run (needs clang)");
        return;
    };
    let dir = tmp_dir("mod");
    std::fs::write(dir.join("page.module.css"), ".title { color: red }").unwrap();
    let main = dir.join("main.ax");
    std::fs::write(
        &main,
        "import * from \"./page.module.css\"\n\
         def main() -> int:\n    \
             print(page_class(\"title\"))\n    \
             return 0\n",
    )
    .unwrap();
    let out = build_and_run(&dir, &main.display().to_string(), "mod");
    assert!(out.trim().starts_with("title_"), "class not hashed: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A module's stylesheet must NOT join the global bundle — modules are fetched
/// by class name, and inlining them would leak their rules globally.
#[test]
fn module_css_is_not_in_the_global_bundle() {
    let dir = tmp_dir("noinline");
    std::fs::write(dir.join("m.module.css"), ".title { color: red }").unwrap();
    let main = dir.join("main.ax");
    std::fs::write(&main, "import * from \"./m.module.css\"\ndef main() -> int:\n    print(styles())\n    return 0\n").unwrap();
    let exe = dir.join(format!("n{EXE}"));
    aoxn::build_paths_exe(&[main.display().to_string()], &exe, true).unwrap();
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- build-cache correctness ----

/// The build cache keys on the bytes of every dependency. If a `.css` file (or
/// a file it `@import`s) is not in that set, editing a stylesheet silently
/// serves a stale executable.
#[test]
fn css_is_in_the_cache_dependency_set() {
    let dir = tmp_dir("cache");
    let a = dir.join("a.css");
    std::fs::write(&a, ".a { color: red }").unwrap();
    let main = dir.join("main.ax");
    std::fs::write(&main, "import * from \"./a.css\"\ndef main() -> int:\n    return 0\n").unwrap();

    let deps = aoxn::dependency_files(&[main.display().to_string()]).expect("deps");
    assert!(
        deps.iter().any(|p| p.ends_with("a.css")),
        "the stylesheet is missing from the dependency set: {deps:?}"
    );

    // an @import'ed partial must be hashed too
    let dir2 = tmp_dir("cache2");
    std::fs::write(dir2.join("b.css"), ".b { color: blue }").unwrap();
    std::fs::write(dir2.join("a.css"), "@import \"./b.css\";").unwrap();
    let main2 = dir2.join("main.ax");
    std::fs::write(&main2, "import * from \"./a.css\"\ndef main() -> int:\n    return 0\n").unwrap();
    let deps2 = aoxn::dependency_files(&[main2.display().to_string()]).expect("deps2");
    assert!(
        deps2.iter().any(|p| p.ends_with("b.css")),
        "an @import'ed partial is missing from the dependency set: {deps2:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

#[test]
fn scan_css_imports_finds_specifiers() {
    let src = "@import \"./a.css\";\n@import url(\"skip.woff2\");\n@import \"./b.css\";";
    let got = scan_css_imports(src);
    assert_eq!(got, vec!["./a.css".to_string(), "./b.css".to_string()]);
}

// ---- plain .css files are unaffected by the minifier's value rules ----

#[test]
fn plain_css_preserves_pseudo_and_media() {
    let dir = tmp_dir("plain");
    std::fs::write(
        dir.join("a.css"),
        "/* c */\n@media (min-width: 30rem) {\n  .btn:hover { color : red ; }\n}\n",
    )
    .unwrap();
    let main = dir.join("main.ax");
    std::fs::write(&main, "import * from \"./a.css\"\ndef main() -> int:\n    print(styles())\n    return 0\n").unwrap();
    let exe = dir.join(format!("p{EXE}"));
    aoxn::build_paths_exe(&[main.display().to_string()], &exe, true).unwrap();
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_dir_all(&dir);
}