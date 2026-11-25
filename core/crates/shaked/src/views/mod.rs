//! A world's declarative views (`shakedown-view/1`, files its `world.yaml` lists): each screen's
//! panels evaluated against the parts' docs at a point of a branch into compact rows for the one
//! generic renderer in the UI. Nothing here knows any world.

pub mod eval;
mod map;
pub mod query;
pub mod source;

pub use eval::{parts_of, screen, Ctx, FeedEvent};
pub use source::{read, record, At, Fallback, Moment, NotLive, Read};
