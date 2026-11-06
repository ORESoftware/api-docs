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
    // Async generators may complete in a different order from source edits.
    // Explore a late artifact from an older authored authority revision.
    if s.authority > 1 {
        for older in 1..s.authority {
            if s.rest != Some(older) {
                out.push(State { rest: Some(older), published: false, ..s });
            }
            if s.graphql != Some(older) {
                out.push(State { graphql: Some(older), published: false, ..s });
            }
        }
    }
    out
}
fn main() {
    let initial = State { authority: 1, rest: None, graphql: None, published: false };
    let mut seen = HashSet::from([initial]);
    let mut queue = VecDeque::from([initial]);
    let mut stale_witness = false;
    let mut republished_witness = false;
    let mut rest_current_graphql_stale = false;
    let mut graphql_current_rest_stale = false;
    let mut late_generation_witness = false;
    while let Some(s) = queue.pop_front() {
        assert!(!s.published || publishable(s),
            "published RPC surface has stale GraphQL/REST projection");
        if (s.rest.is_some() || s.graphql.is_some()) && !publishable(s) {
            stale_witness = true;
        }
        if s.published && s.authority == 3 { republished_witness = true; }
        if s.rest == Some(s.authority) && s.graphql != Some(s.authority) {
            assert!(!publishable(s), "REST-only admission is unsafe");
            rest_current_graphql_stale = true;
        }
        if s.graphql == Some(s.authority) && s.rest != Some(s.authority) {
            assert!(!publishable(s), "GraphQL-only admission is unsafe");
            graphql_current_rest_stale = true;
        }
        for c in next(s) {
            if c.authority == s.authority && c.authority > 1
                && (c.rest.is_some_and(|revision| revision < c.authority)
                    || c.graphql.is_some_and(|revision| revision < c.authority))
            {
                assert!(!c.published, "late stale generator result retained publication");
                late_generation_witness = true;
            }
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
    assert!(stale_witness && republished_witness
        && rest_current_graphql_stale && graphql_current_rest_stale
        && late_generation_witness,
        "independent lane staleness, late generators and recovery must all be reachable");
    assert!(seen.len() > 10, "vacuous projection state space");
    println!("api-docs formal projection model: {} states", seen.len());
}
