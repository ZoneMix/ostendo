//! Markdown presentation parsing.

mod inline;
pub mod parser;
mod regex_patterns;
mod split;
mod tables;

pub use parser::parse_presentation;
