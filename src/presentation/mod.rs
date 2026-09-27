//! Presentation data structures and session state persistence.

pub mod rehearsal;
mod slide;
mod state;

pub use slide::*;
pub use state::StateManager;
