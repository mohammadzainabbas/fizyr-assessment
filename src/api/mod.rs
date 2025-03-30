//! Provides clients and utilities for interacting with external APIs.
//!
//! Includes:
//! - `openaq`: Client for the real OpenAQ API.
//! - `mock`: A mock data provider used as a fallback or for testing.

mod mock;
mod openaq;

pub use mock::*;
pub use openaq::*;
