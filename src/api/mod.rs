//! Provides clients and utilities for interacting with external APIs.
//!
//! Includes:
//! - `openaq`: Client for the real OpenAQ API.
// Removed mock module description

// mod mock; // Removed mock module
mod openaq;
// Removed mock test module reference if it existed implicitly

// pub use mock::*; // Removed mock re-export
pub use openaq::*;
