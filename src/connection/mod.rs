//! Connection management
//!
//! Handles connection state, lifecycle, and tracking.

mod manager;
mod state;

pub use manager::{ConnectionManager, ConnectionManagerConfig};
pub use state::{ConnectionState, ConnectionId};

