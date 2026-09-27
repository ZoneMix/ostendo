//! Rendering engine, animation system, and terminal output pipeline.

pub mod animation;
mod engine;
pub mod layout;
pub mod text;

pub use engine::{overflowing_slides, Presenter, PresenterConfig};
