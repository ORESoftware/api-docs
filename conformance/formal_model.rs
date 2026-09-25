//! Bounded model checker for API contract operation admission.
//! Once an operation_id is admitted with a behavioral digest, later admission is
//! idempotent only for the same digest; a conflicting digest is not a transition.

use std::collections::{HashSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct State {
    op_a_digest: Option<u8>,
    op_b_digest: Option<u8>,
}

fn invariant(s: State) -> bool {
    [s.op_a_digest, s.op_b_digest]
        .into_iter()
        .flatten()
        .all(|digest| matches!(digest, 1 | 2))
}

fn admit(current: Option<u8>, digest: u8) -> Option<Option<u8>> {
    match current {
        None => Some(Some(digest)),
        Some(existing) if existing == digest => Some(current),
        Some(_) => None,
    }
}

fn next(s: State) -> Vec<State> {
    let mut out = Vec::new();
    for digest in [1, 2] {
        if let Some(value) = admit(s.op_a_digest, digest) {
            out.push(State { op_a_digest: value, ..s });
        }
        if let Some(value) = admit(s.op_b_digest, digest) {
            out.push(State { op_b_digest: value, ..s });
        }
    }
    out
}

fn main() {
    let initial = State { op_a_digest: None, op_b_digest: None };
    let mut seen = HashSet::from([initial]);
    let mut queue = VecDeque::from([initial]);
    while let Some(state) = queue.pop_front() {
        assert!(invariant(state), "invalid contract registry state: {state:?}");
        for candidate in next(state) {
            assert!(invariant(candidate), "unsafe contract transition: {state:?} -> {candidate:?}");
            if let Some(old) = state.op_a_digest { assert_eq!(candidate.op_a_digest, Some(old)); }
            if let Some(old) = state.op_b_digest { assert_eq!(candidate.op_b_digest, Some(old)); }
            if seen.insert(candidate) { queue.push_back(candidate); }
        }
    }
    assert!(admit(Some(1), 2).is_none(), "conflicting behavioral digest was admitted");
    assert!(admit(Some(2), 1).is_none(), "conflicting behavioral digest was admitted");
    println!("api-docs formal model: explored {} states; invariants hold", seen.len());
}
