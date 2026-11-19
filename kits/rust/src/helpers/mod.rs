//! Helpers for common kinds of integration. Most sit behind a cargo feature.
pub mod args;
pub mod bench;
pub mod busy;
pub mod clock;
#[cfg(feature = "compose")]
pub mod compose;
pub mod docker_name;
#[cfg(feature = "folder")]
pub mod folder;
pub mod forwarded;
#[cfg(feature = "http")]
pub mod frappe;
#[cfg(feature = "http")]
pub mod http;
pub mod instances;
#[cfg(feature = "mariadb")]
pub mod mariadb;
#[cfg(feature = "postgres")]
pub mod pg;
mod pool;
pub mod process_group;
#[cfg(feature = "folder")]
pub mod save_dirs;
pub mod sql_tools;
#[cfg(feature = "compose")]
pub mod tar_digest;
#[cfg(feature = "folder")]
pub mod temp_dir;
mod tree;
#[cfg(feature = "http")]
pub mod twenty;
pub mod world_time;

pub use pool::blocking;
pub use tree::{remove_tree, remove_tree_or_log};
