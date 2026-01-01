//! Request routing
//!
//! Routes requests based on target and policy.

mod dispatcher;
mod policy;

pub use dispatcher::RequestRouter;
pub use policy::RoutingPolicy;

