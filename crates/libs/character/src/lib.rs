//! The shared library for the character server and clients.
//!
//! This crate contains all the shared logic, including the communication
//! protocol, error types, and the `CharacterClient` for interacting with
//! the server.

pub mod client;
pub mod error;
pub mod protocol;

pub use client::CharacterClient;
pub use error::CharacterError;
pub use protocol::{Notification, Request, Response, ServerMessage, StatBar};
