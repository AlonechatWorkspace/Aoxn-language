//! TS-M1 front end: TypeScript syntax compiled through the existing
//! Aoxn pipeline (docs/ts-m1-spec.md). S0 = lexer, S1 = parser lowering
//! to the shared `crate::ast`.

pub mod lexer;
pub mod parser;
