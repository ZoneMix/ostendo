//! Markdown-to-slide parsing pipeline with directive support.

pub mod inline;
pub mod parser;
pub mod regex_patterns;
mod split;
pub mod tables;

pub use parser::parse_presentation;
