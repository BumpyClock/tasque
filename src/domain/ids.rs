use crate::types::State;
use once_cell::sync::Lazy;
use rand::RngExt;
use regex::Regex;
use std::collections::HashSet;

static SEQUENTIAL_ROOT_ID: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^tsq-[1-9][0-9]*$").expect("sequential root id regex"));

/// Crockford base32 alphabet (lowercase, excluding i, l, o, u).
/// Matches the accepted legacy random root id shape `tsq-<8 crockford chars>`.
const CROCKFORD_ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
const RANDOM_ID_LEN: usize = 8;

fn mint_random_canonical_id() -> String {
    let mut rng = rand::rng();
    let mut buf = [0u8; RANDOM_ID_LEN];
    for slot in buf.iter_mut() {
        let idx = rng.random_range(0..CROCKFORD_ALPHABET.len());
        *slot = CROCKFORD_ALPHABET[idx];
    }
    format!("tsq-{}", std::str::from_utf8(&buf).expect("ascii"))
}

/// Mint a flat random canonical task id (`tsq-<8 crockford chars>`) that does
/// not collide with any existing task id. Roots and children share this shape;
/// parentage lives in `parent_id`, not in the id text. Old sequential
/// `tsq-<number>` and `parent.N` ids remain valid/readable.
pub fn make_task_id(state: &State) -> String {
    loop {
        let candidate = mint_random_canonical_id();
        if !state.tasks.contains_key(&candidate) {
            return candidate;
        }
    }
}

/// Batch-friendly flat random id allocator. Reserves generated ids against
/// existing state and ids minted earlier in the same batch so a single write
/// lock can produce a consistent set of new tasks.
pub struct TaskIdAllocator {
    reserved_ids: HashSet<String>,
}

impl TaskIdAllocator {
    pub fn new(state: &State) -> Self {
        Self {
            reserved_ids: state.tasks.keys().cloned().collect(),
        }
    }

    pub fn next_id(&mut self) -> String {
        loop {
            let candidate = mint_random_canonical_id();
            if self.reserved_ids.insert(candidate.clone()) {
                return candidate;
            }
        }
    }
}

pub fn is_valid_root_id(raw: &str) -> bool {
    is_sequential_root_id(raw) || is_legacy_random_root_id(raw)
}

pub fn is_sequential_root_id(raw: &str) -> bool {
    SEQUENTIAL_ROOT_ID.is_match(raw)
}

pub fn is_legacy_random_root_id(raw: &str) -> bool {
    let Some(rest) = raw.strip_prefix("tsq-") else {
        return false;
    };
    rest.len() == 8
        && rest.chars().all(|ch| {
            matches!(
                ch,
                '0'..='9' | 'a'..='h' | 'j'..='k' | 'm'..='n' | 'p'..='t' | 'v'..='z'
            )
        })
}
