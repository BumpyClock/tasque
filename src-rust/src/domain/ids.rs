use crate::types::State;
use rand::RngCore;
use rand::rngs::OsRng;

const CROCKFORD: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

pub fn make_root_id(_title: Option<&str>, _nonce: Option<&str>) -> String {
    let mut bytes = [0u8; 5];
    OsRng.fill_bytes(&mut bytes);
    let mut id = String::with_capacity(8);
    let mut bits = 0u32;
    let mut acc = 0u64;
    for byte in bytes {
        acc = (acc << 8) | byte as u64;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let index = ((acc >> bits) & 0x1f) as usize;
            id.push(CROCKFORD[index] as char);
        }
    }
    format!("tsq-{}", id)
}

pub fn next_child_id(state: &State, parent_id: &str) -> String {
    let max_child = state.child_counters.get(parent_id).copied().unwrap_or(0);
    format!("{}.{}", parent_id, max_child + 1)
}
