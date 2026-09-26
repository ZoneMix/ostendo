//! Rendering engine, animation system, and terminal output pipeline.

pub mod animation;
mod engine;
pub mod layout;
mod progress;
pub mod text;

pub use engine::Presenter;
pub use engine::PresenterConfig;
