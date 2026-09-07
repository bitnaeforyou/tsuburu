//! Dialogue search: recognised page text, the work queue, and matching.
//!
//! Nothing here touches the network or an OCR engine. The grinder that
//! downloads pages and runs recognition lives in the server, feeding
//! results into [`DialogueStore`].

pub mod jamo;
pub mod matcher;
pub mod shard;
pub mod store;

pub use matcher::{Match, Query};
pub use shard::{Shard, ShardEntry, ShardError};
pub use store::{Counts, DialogueError, DialogueStore, Hit, JobRecord, PageText, Priority, Status};
