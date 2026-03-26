//! Markdown-to-slide parsing pipeline with directive support.

pub mod parser;
pub mod regex_patterns;
pub mod tables;
pub mod inline;

pub use parser::parse_presentation;
