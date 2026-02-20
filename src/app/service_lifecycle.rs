#[path = "service_lifecycle_claim.rs"]
mod service_lifecycle_claim;
#[path = "service_lifecycle_helpers.rs"]
mod service_lifecycle_helpers;
#[path = "service_lifecycle_links.rs"]
mod service_lifecycle_links;
#[path = "service_lifecycle_merge.rs"]
mod service_lifecycle_merge;

pub use service_lifecycle_claim::{claim, close, duplicate, reopen, supersede};
pub use service_lifecycle_links::{dep_add, dep_remove, link_add, link_remove};
pub use service_lifecycle_merge::{duplicate_candidates, merge};
