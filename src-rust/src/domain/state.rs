use crate::types::State;

pub fn create_empty_state() -> State {
    State {
        tasks: std::collections::HashMap::new(),
        deps: std::collections::HashMap::new(),
        links: std::collections::HashMap::new(),
        child_counters: std::collections::HashMap::new(),
        created_order: Vec::new(),
        applied_events: 0,
    }
}
