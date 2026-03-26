//! Rendering engine, animation system, and terminal output pipeline.

mod engine;
pub mod animation;
pub mod text;
pub mod layout;
mod progress;

pub use engine::Presenter;
pub use engine::PresenterConfig;
