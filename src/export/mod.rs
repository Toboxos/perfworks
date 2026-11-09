//! Export formats.
//!
//! Each submodule turns a [`crate::model::Project`] into one external representation and
//! owns nothing else: no file dialogs, no app state, no rendering decisions. That keeps
//! them testable without a window, which matters because neither format can be eyeballed
//! from inside the test suite.

pub mod image;
pub mod step;
