//! Finite safety model for one authored RPC contract and two derived surfaces.
//! A new authored revision invalidates publication even when older projections
//! remain cached. No claim is made about production generator correctness.
use std::collections::{HashSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct State {
    authority: u8,
    rest: Option<u8>,
    graphql: Option<u8>,
    published: bool,
}
fn publishable(s: State) -> bool {
    s.rest == Some(s.authority) && s.graphql == Some(s.authority)
}
fn next(s: State) -> Vec<State> {
    let mut out = Vec::new();
    if s.authority < 3 {
        out.push(State { authority: s.authority + 1, published: false, ..s });
    }
    if s.rest != Some(s.authority) {
        out.push(State { rest: Some(s.authority), published: false, ..s });
    }
    if s.graphql != Some(s.authority) {
        out.push(State { graphql: Some(s.authority), published: false, ..s });
    }
    if !s.published && publishable(s) {
        out.push(State { published: true, ..s });
    }
    out
}
fn main() {
    let initial = State { authority: 1, rest: None, graphql: None, published: false };
    let mut seen = HashSet::from([initial]);
    let mut queue = VecDeque::from([initial]);
    let mut stale_witness = false;
    let mut republished_witness = false;
    while let Some(s) = queue.pop_front() {
        assert!(!s.published || publishable(s),
            "published RPC surface has stale GraphQL/REST projection");
        if (s.rest.is_some() || s.graphql.is_some()) && !publishable(s) {
            stale_witness = true;
        }
        if s.published && s.authority == 3 { republished_witness = true; }
        for c in next(s) {
            assert!(c.authority == s.authority || c.authority == s.authority + 1);
            if c.authority > s.authority {
                assert!(!c.published, "authored revision must revoke publication");
            }
            if c.published {
                assert!(publishable(c), "mismatched projection published");
            }
            if seen.insert(c) { queue.push_back(c); }
        }
    }
    assert!(stale_witness && republished_witness,
        "projection drift/recovery witnesses must be reachable");
    assert!(seen.len() > 10, "vacuous projection state space");
    println!("api-docs formal projection model: {} states", seen.len());
}
