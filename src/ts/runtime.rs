//! TS-M1 runtime helpers.
//!
//! The TypeScript front end lowers `number` to f64 and needs a few helpers
//! the Aoxn surface does not have: JS `ToInt32` bit semantics for the
//! bitwise operators (`& | ^ ~ << >> >>>`) and a JS-flavored
//! number-to-string for `console.log`. They are written here as ordinary
//! Aoxn source (using the `to_int()`/`to_float()` conversion builtins) and parsed
//! through the regular Aoxn front end at TS compile time, then merged into
//! the program's function list — only when the program actually references
//! them. Names are `__ts_`-prefixed to stay out of user space.
//!
//! Known deviations from real JS (documented in docs/ts-m1-spec.md):
//! - `NaN`/`Infinity` print as `nan`/`inf` (the underlying `str(float)`
//!   is `%f`); large magnitudes print fixed-point, not `1e+21`.
//! - float formatting is `%f` with trailing zeros trimmed, so round-trip
//!   shortest-form (JS's `0.30000000000000004`) shows 6 decimals instead.

use crate::ast::FnDecl;
use crate::Diag;

pub const TS_RUNTIME_SRC: &str = r#"
extern def fmod(a: float, b: float) -> float

def __ts_mod(a: float, b: float) -> float:
    return fmod(a, b)

def __ts_tou32(x: float) -> int:
    n = to_int(x)
    n = n % 4294967296
    if n < 0:
        n = n + 4294967296
    return n

def __ts_fromu32(u: int) -> float:
    if u >= 2147483648:
        return to_float(u - 4294967296)
    return to_float(u)

def __ts_pow2(s: int) -> int:
    m = 1
    i = 0
    while i < s:
        m = m * 2
        i = i + 1
    return m

def __ts_band(a: float, b: float) -> float:
    x = __ts_tou32(a)
    y = __ts_tou32(b)
    r = 0
    m = 1
    i = 0
    while i < 32:
        if x / m % 2 == 1 and y / m % 2 == 1:
            r = r + m
        m = m * 2
        i = i + 1
    return __ts_fromu32(r)

def __ts_bor(a: float, b: float) -> float:
    x = __ts_tou32(a)
    y = __ts_tou32(b)
    r = 0
    m = 1
    i = 0
    while i < 32:
        if x / m % 2 == 1 or y / m % 2 == 1:
            r = r + m
        m = m * 2
        i = i + 1
    return __ts_fromu32(r)

def __ts_bxor(a: float, b: float) -> float:
    x = __ts_tou32(a)
    y = __ts_tou32(b)
    r = 0
    m = 1
    i = 0
    while i < 32:
        if (x / m % 2 == 1) != (y / m % 2 == 1):
            r = r + m
        m = m * 2
        i = i + 1
    return __ts_fromu32(r)

def __ts_bnot(x: float) -> float:
    return __ts_fromu32(4294967295 - __ts_tou32(x))

def __ts_shl(a: float, b: float) -> float:
    s = __ts_tou32(b) % 32
    x = __ts_tou32(a)
    m = __ts_pow2(s)
    return __ts_fromu32(x * m % 4294967296)

def __ts_sar(a: float, b: float) -> float:
    s = __ts_tou32(b) % 32
    n = to_int(a)
    d = __ts_pow2(s)
    q = n / d
    if n < 0 and n % d != 0:
        q = q - 1
    return to_float(q)

def __ts_shr(a: float, b: float) -> float:
    s = __ts_tou32(b) % 32
    d = __ts_pow2(s)
    return to_float(__ts_tou32(a) / d)

def __ts_num(v: float) -> string:
    i = to_int(v)
    if to_float(i) == v:
        return str(i)
    s = str(v)
    n = len(s)
    while n > 1 and load_u8(s, n - 1) == 48:
        n = n - 1
    if n > 1 and load_u8(s, n - 1) == 46:
        n = n - 1
    store_u8(s, n, 0)
    return s
"#;

/// parse the runtime source through the regular Aoxn front end
pub fn runtime_funcs() -> Result<Vec<FnDecl>, Diag> {
    let fid = crate::files::register("<ts-runtime>");
    let tokens = crate::lexer::lex(TS_RUNTIME_SRC, fid)?;
    let prog = crate::parser::parse(tokens)?;
    Ok(prog.funcs)
}
