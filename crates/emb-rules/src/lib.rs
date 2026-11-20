//! Lexical rule engine for embedded-systems standards.
//!
//! Pure functions over file paths and contents: no I/O, no floats, no
//! recursion, so the same code runs natively under `cargo test` and inside
//! the Aram WASM sandbox.

pub mod c_subset;
pub mod cpp_subset;
pub mod engine;
pub mod hardware;
pub mod lex;
pub mod model;
pub mod profile;
pub mod resources;
pub mod runtime;
pub mod security;

pub use engine::{check, deviatable, render, rule_by_id};
pub use model::{Category, Finding, Pack, Rule};
pub use profile::Profile;
