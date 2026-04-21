mod create;
mod lifecycle;
mod query;
mod relationship;
mod update;

pub use create::{CreateArgs, execute_create};
pub use lifecycle::{CloseArgs, ReopenArgs, execute_close, execute_reopen};
pub use query::{
    ListArgs, ReadyArgs, SearchArgs, ShowArgs, StaleArgs, execute_list, execute_ready,
    execute_search, execute_show, execute_stale,
};
pub use relationship::{
    DuplicateArgs, DuplicatesArgs, MergeArgs, SupersedeArgs, execute_duplicate, execute_duplicates,
    execute_merge, execute_supersede,
};
use serde::Serialize;
pub use update::{UpdateArgs, execute_update};

#[derive(Debug, Serialize)]
pub struct TaskJson<T> {
    pub task: T,
}
