//! Provides database interaction functionalities.
//!
//! Currently, this module focuses on PostgreSQL interactions via the `postgres` submodule.

mod postgres;

pub use postgres::*;
