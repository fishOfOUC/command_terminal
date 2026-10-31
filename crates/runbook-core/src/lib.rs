pub mod error;
pub mod model;
pub mod service;
pub mod shell;
pub mod store;
pub mod terminal;
mod terminal_protocol;

pub use error::{ApiError, Result};
pub use service::{dispatch, Request};
pub use store::Store;
