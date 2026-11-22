//! The link between the control box and a `shaked worker` running a branch in a sandbox.

pub mod frames;
pub mod link;
pub mod links;
pub mod tickets;

pub use link::router;
pub use links::Links;
pub use tickets::{Ticket, Tickets};
