// app/mod.rs - Application State and Configuration
//
// This module contains the core application state that is platform-independent.
// All UI and platform-specific code should reference this state.

mod state;

pub use state::*;
