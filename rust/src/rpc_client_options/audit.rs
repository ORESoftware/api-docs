//! Static audit of RPC client call sites against the api-docs option catalog.
//!
//! In Rust and TypeScript the type-state builders make a contradictory chain a
//! compile error. In plain JavaScript, Dart without sound checking, or any
//! hand-rolled caller there is no compiler to lean on, and a chain that spends
//! an exclusive group twice or calls a stream option on a unary chain is only
//! discovered at runtime — if at all.
//!
//! `ores-stack` already walks consumer repositories, so it is the right place
//! to catch that statically. The catalog travels embedded in the pinned
//! `ores-api-docs` crate, so an audit is always against the same surface the
//! consumer's generated clients were produced from.
//!
//! The audit is deliberately conservative: it only inspects chains that begin
//! at a recognized client entry point, and it reports an unknown method rather
//! than guessing, so a false negative is possible but a false positive is not.

use super::model::{AppliesTo, Arity, Catalog, Option_};
use super::names::Language;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Entry points that begin an auditable chain.
const CHAIN_ENTRIES: &[&str] = &["prepare", "call"];

/// What kind of contract violation a call site exhibits.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SurfaceIssueKind {
    /// A method that the catalog does not declare for any surface.
    UnknownOption { method: String },
    /// A method declared only for the other client surface.
    WrongSurface {
        method: String,
        declared_for: String,
        used_on: String,
    },
    /// Two members of one exclusive group in a single chain.
    ContradictoryOptions {
        group: String,
        first: String,
        second: String,
    },
    /// A non-repeatable option applied more than once.
    RepeatedOption { method: String },
    /// A chain that never reaches a terminal, so it never opens the network.
    UnterminatedChain,
    /// A chain that ends in the other surface's terminal.
    WrongTerminal { terminal: String, used_on: String },
}

impl fmt::Display for SurfaceIssueKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOption { method } => {
                write!(formatter, "`{method}` is not an option in the catalog")
            }
            Self::WrongSurface {
                method,
                declared_for,
                used_on,
            } => write!(
                formatter,
                "`{method}` is {declared_for}-only but this chain is {used_on}"
            ),
            Self::ContradictoryOptions {
                group,
                first,
                second,
            } => write!(
                formatter,
                "`{first}` and `{second}` are contradictory members of `{group}`"
            ),
            Self::RepeatedOption { method } => {
                write!(formatter, "`{method}` may only be applied once per chain")
            }
            Self::UnterminatedChain => write!(
                formatter,
                "this chain never reaches a terminal, so no call is made"
            ),
            Self::WrongTerminal { terminal, used_on } => write!(
                formatter,
                "`{terminal}` does not terminate a {used_on} chain"
            ),
        }
    }
}

/// One violation, located well enough to fix.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SurfaceIssue {
    pub line: usize,
    pub kind: SurfaceIssueKind,
}

impl fmt::Display for SurfaceIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.kind)
    }
}

/// Result of auditing one source file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SurfaceReport {
    pub chains_inspected: usize,
    pub issues: Vec<SurfaceIssue>,
}

impl SurfaceReport {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Catalog lookups keyed by the method spelling of one language.
struct SurfaceIndex<'a> {
    by_method: BTreeMap<String, &'a Option_>,
    unary_terminal: BTreeSet<String>,
    stream_terminal: BTreeSet<String>,
    plan_only: BTreeSet<String>,
}

impl<'a> SurfaceIndex<'a> {
    fn build(catalog: &'a Catalog, language: Language) -> Self {
        let mut by_method = BTreeMap::new();
        let mut unary_terminal = BTreeSet::new();
        let mut stream_terminal = BTreeSet::new();
        let mut plan_only = BTreeSet::new();

        for option in &catalog.options {
            let method = language.method_name(&option.option_id);
            if option.terminal {
                // to_plan is on both surfaces and opens nothing.
                if option.applies_to == AppliesTo::Both {
                    plan_only.insert(method.clone());
                } else if option.applies_to.on_unary() {
                    unary_terminal.insert(method.clone());
                } else {
                    stream_terminal.insert(method.clone());
                }
            }
            by_method.insert(method, option);
        }

        Self {
            by_method,
            unary_terminal,
            stream_terminal,
            plan_only,
        }
    }
}

/// Audit one source file's RPC chains against the catalog.
///
/// `language` selects the method spelling; the catalog is the authority for
/// which methods exist, which surface each belongs to, and which contradict.
#[must_use]
pub fn audit_source(source: &str, catalog: &Catalog, language: Language) -> SurfaceReport {
    let index = SurfaceIndex::build(catalog, language);
    let mut report = SurfaceReport::default();

    for chain in extract_chains(source, language) {
        report.chains_inspected += 1;
        audit_chain(&chain, &index, &mut report);
    }
    report.issues.sort();
    report
}

/// One `.prepare(...)....terminal()` chain, as method names plus a line number.
#[derive(Debug)]
struct Chain {
    line: usize,
    methods: Vec<String>,
}

/// Pull chains out of source text without a full parser for every language.
///
/// A chain starts at a recognized entry point and continues through `.name(`
/// segments. It ends at a terminal, at a statement boundary, or when a segment
/// is not a bare method call. Anything ambiguous simply ends the chain, which
/// can lose a violation but cannot invent one.
fn extract_chains(source: &str, language: Language) -> Vec<Chain> {
    let entries: BTreeSet<String> = CHAIN_ENTRIES
        .iter()
        .map(|entry| language.method_name(entry))
        .collect();

    let characters: Vec<char> = source.chars().collect();
    // Line number for every character offset, computed once.
    let mut line_at = Vec::with_capacity(characters.len() + 1);
    let mut line = 1_usize;
    for character in &characters {
        line_at.push(line);
        if *character == '\n' {
            line += 1;
        }
    }
    line_at.push(line);

    let mut chains = Vec::new();
    let mut index = 0_usize;

    while index < characters.len() {
        if characters[index] != '.' {
            index += 1;
            continue;
        }
        let Some((name, arguments_at)) = read_method(&characters, index) else {
            index += 1;
            continue;
        };
        if !entries.contains(&name) {
            index += 1;
            continue;
        }

        // An entry point. Walk the links that follow it.
        let chain_line = line_at[index];
        let mut methods = Vec::new();
        let mut cursor = skip_call_arguments(&characters, arguments_at);

        while let Some(position) = cursor {
            let mut scan = position;
            while scan < characters.len() && characters[scan].is_whitespace() {
                scan += 1;
            }
            if characters.get(scan) != Some(&'.') {
                break;
            }
            let Some((next_name, next_arguments_at)) = read_method(&characters, scan) else {
                break;
            };
            methods.push(next_name);
            cursor = skip_call_arguments(&characters, next_arguments_at);
        }

        if !methods.is_empty() {
            chains.push(Chain {
                line: chain_line,
                methods,
            });
        }
        // Resume after the chain, or just past the entry if it did not parse.
        index = cursor.unwrap_or(arguments_at).max(index + 1);
    }
    chains
}

/// Read `.name` at `position`, returning the name and the index just past it.
fn read_method(bytes: &[char], position: usize) -> Option<(String, usize)> {
    debug_assert_eq!(bytes.get(position), Some(&'.'));
    let mut cursor = position + 1;
    let mut name = String::new();
    while cursor < bytes.len() && (bytes[cursor].is_alphanumeric() || bytes[cursor] == '_') {
        name.push(bytes[cursor]);
        cursor += 1;
    }
    if name.is_empty() {
        return None;
    }
    // Only a call is a chain link; `.field` is not.
    let mut probe = cursor;
    while probe < bytes.len() && bytes[probe].is_whitespace() {
        probe += 1;
    }
    if bytes.get(probe) != Some(&'(') {
        return None;
    }
    Some((name, probe))
}

/// Skip a balanced `( ... )` starting at `position`, honouring string literals.
fn skip_call_arguments(bytes: &[char], position: usize) -> Option<usize> {
    if bytes.get(position) != Some(&'(') {
        return None;
    }
    let mut depth = 0_i32;
    let mut cursor = position;
    let mut quote: Option<char> = None;
    while cursor < bytes.len() {
        let character = bytes[cursor];
        if let Some(open) = quote {
            if character == '\\' {
                cursor += 2;
                continue;
            }
            if character == open {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        match character {
            '"' | '\'' | '`' => quote = Some(character),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(cursor + 1);
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn audit_chain(chain: &Chain, index: &SurfaceIndex<'_>, report: &mut SurfaceReport) {
    // The surface is inferred from the terminal, because that is the only link
    // that is unambiguous. A chain with no terminal is reported as such.
    let terminal = chain.methods.iter().find(|method| {
        index.unary_terminal.contains(*method) || index.stream_terminal.contains(*method)
    });

    let surface = match terminal {
        Some(method) if index.unary_terminal.contains(method) => AppliesTo::Unary,
        Some(_) => AppliesTo::Stream,
        None => {
            // A chain ending in to_plan is complete without opening a socket.
            if chain
                .methods
                .iter()
                .any(|method| index.plan_only.contains(method))
            {
                AppliesTo::Both
            } else {
                report.issues.push(SurfaceIssue {
                    line: chain.line,
                    kind: SurfaceIssueKind::UnterminatedChain,
                });
                return;
            }
        }
    };

    let mut spent_groups: BTreeMap<String, String> = BTreeMap::new();
    let mut applied: BTreeSet<String> = BTreeSet::new();

    for method in &chain.methods {
        if index.plan_only.contains(method) {
            continue;
        }
        let Some(option) = index.by_method.get(method) else {
            report.issues.push(SurfaceIssue {
                line: chain.line,
                kind: SurfaceIssueKind::UnknownOption {
                    method: method.clone(),
                },
            });
            continue;
        };

        if option.terminal {
            let terminates_unary = index.unary_terminal.contains(method);
            let matches_surface = match surface {
                AppliesTo::Unary => terminates_unary,
                AppliesTo::Stream => !terminates_unary,
                AppliesTo::Both => true,
            };
            if !matches_surface {
                report.issues.push(SurfaceIssue {
                    line: chain.line,
                    kind: SurfaceIssueKind::WrongTerminal {
                        terminal: method.clone(),
                        used_on: surface.as_str().to_owned(),
                    },
                });
            }
            continue;
        }

        let reachable = match surface {
            AppliesTo::Unary => option.applies_to.on_unary(),
            AppliesTo::Stream => option.applies_to.on_stream(),
            AppliesTo::Both => true,
        };
        if !reachable {
            report.issues.push(SurfaceIssue {
                line: chain.line,
                kind: SurfaceIssueKind::WrongSurface {
                    method: method.clone(),
                    declared_for: option.applies_to.as_str().to_owned(),
                    used_on: surface.as_str().to_owned(),
                },
            });
            continue;
        }

        if let Some(group) = option.exclusive_group.as_deref() {
            if let Some(first) = spent_groups.get(group) {
                report.issues.push(SurfaceIssue {
                    line: chain.line,
                    kind: SurfaceIssueKind::ContradictoryOptions {
                        group: group.to_owned(),
                        first: first.clone(),
                        second: method.clone(),
                    },
                });
                continue;
            }
            spent_groups.insert(group.to_owned(), method.clone());
        }

        if option.arity == Arity::Once && !applied.insert(method.clone()) {
            report.issues.push(SurfaceIssue {
                line: chain.line,
                kind: SurfaceIssueKind::RepeatedOption {
                    method: method.clone(),
                },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::embedded().expect("embedded catalog")
    }

    fn audit(source: &str) -> SurfaceReport {
        audit_source(source, &catalog(), Language::TypeScript)
    }

    #[test]
    fn a_correct_unary_chain_is_clean() {
        let report = audit(
            r#"
            const [user, ctx] = await client
              .prepare("demo.users.find_user")
              .addPathField("user_id", id)
              .useMessagePack()
              .withTimeout(2000)
              .withRetries(3)
              .makeCall();
            "#,
        );
        assert_eq!(report.chains_inspected, 1);
        assert!(report.is_clean(), "{:?}", report.issues);
    }

    #[test]
    fn a_correct_streaming_chain_is_clean() {
        let report = audit(
            r#"
            const events = await streams
              .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
              .withBackpressure("drop_oldest")
              .withStreamBuffer(256)
              .sampleEach(100)
              .stream();
            "#,
        );
        assert_eq!(report.chains_inspected, 1);
        assert!(report.is_clean(), "{:?}", report.issues);
    }

    #[test]
    fn spending_a_group_twice_is_reported() {
        let report = audit(r#"client.prepare("k").useJson().useProtobuf().makeCall();"#);
        assert_eq!(
            report.issues,
            vec![SurfaceIssue {
                line: 1,
                kind: SurfaceIssueKind::ContradictoryOptions {
                    group: "serialization".to_owned(),
                    first: "useJson".to_owned(),
                    second: "useProtobuf".to_owned(),
                },
            }]
        );
    }

    #[test]
    fn contradictory_auth_and_rate_options_are_reported() {
        let report = audit(r#"client.prepare("k").omitAuth().withBearerToken(t).makeCall();"#);
        assert!(matches!(
            report.issues.first().map(|issue| &issue.kind),
            Some(SurfaceIssueKind::ContradictoryOptions { group, .. }) if group == "auth_mode"
        ));

        let report = audit(r#"client.prepare("k").throttle(10).debounce(10).makeCall();"#);
        assert!(matches!(
            report.issues.first().map(|issue| &issue.kind),
            Some(SurfaceIssueKind::ContradictoryOptions { group, .. }) if group == "rate_limit"
        ));
    }

    #[test]
    fn a_stream_option_on_a_unary_chain_is_reported() {
        let report = audit(r#"client.prepare("k").withBackpressure("buffer").makeCall();"#);
        assert_eq!(
            report.issues,
            vec![SurfaceIssue {
                line: 1,
                kind: SurfaceIssueKind::WrongSurface {
                    method: "withBackpressure".to_owned(),
                    declared_for: "stream".to_owned(),
                    used_on: "unary".to_owned(),
                },
            }]
        );
    }

    #[test]
    fn a_unary_option_on_a_streaming_chain_is_reported() {
        let report = audit(r#"streams.prepare("k", r).dryRun().stream();"#);
        assert_eq!(
            report.issues,
            vec![SurfaceIssue {
                line: 1,
                kind: SurfaceIssueKind::WrongSurface {
                    method: "dryRun".to_owned(),
                    declared_for: "unary".to_owned(),
                    used_on: "stream".to_owned(),
                },
            }]
        );
    }

    #[test]
    fn an_unterminated_chain_is_reported() {
        let report = audit(r#"const pending = client.prepare("k").withTimeout(100);"#);
        assert_eq!(
            report.issues,
            vec![SurfaceIssue {
                line: 1,
                kind: SurfaceIssueKind::UnterminatedChain,
            }]
        );
    }

    #[test]
    fn a_chain_ending_in_to_plan_opens_nothing_and_is_accepted() {
        let report = audit(r#"const plan = client.prepare("k").withTimeout(100).toPlan();"#);
        assert!(report.is_clean(), "{:?}", report.issues);
    }

    #[test]
    fn an_unknown_method_is_reported_rather_than_guessed() {
        let report = audit(r#"client.prepare("k").withMagic(1).makeCall();"#);
        assert_eq!(
            report.issues,
            vec![SurfaceIssue {
                line: 1,
                kind: SurfaceIssueKind::UnknownOption {
                    method: "withMagic".to_owned(),
                },
            }]
        );
    }

    #[test]
    fn repeating_a_once_option_is_reported() {
        let report = audit(r#"client.prepare("k").withTimeout(1).withTimeout(2).makeCall();"#);
        assert_eq!(
            report.issues,
            vec![SurfaceIssue {
                line: 1,
                kind: SurfaceIssueKind::RepeatedOption {
                    method: "withTimeout".to_owned(),
                },
            }]
        );
    }

    #[test]
    fn repeatable_options_may_recur() {
        let report =
            audit(r#"client.prepare("k").addHeader("a", 1).addHeader("b", 2).makeCall();"#);
        assert!(report.is_clean(), "{:?}", report.issues);
    }

    #[test]
    fn the_rust_spelling_is_audited_with_the_rust_surface() {
        let source = r#"
            let plan = UnaryCall::new("k", "/v1/rpc")
                .use_json()
                .use_protobuf()
                .to_plan();
        "#;
        // Rust chains do not start at `prepare`, so nothing is extracted here;
        // the audit reports no chains rather than inventing one.
        let report = audit_source(source, &catalog(), Language::Rust);
        assert_eq!(report.chains_inspected, 0);
        assert!(report.is_clean());

        let via_client = r#"let out = client.prepare("k").use_json().use_protobuf().make_call();"#;
        let report = audit_source(via_client, &catalog(), Language::Rust);
        assert_eq!(report.chains_inspected, 1);
        assert!(matches!(
            report.issues.first().map(|issue| &issue.kind),
            Some(SurfaceIssueKind::ContradictoryOptions { .. })
        ));
    }

    #[test]
    fn source_without_any_rpc_chain_is_not_reported_on() {
        let report = audit("const total = items.map(f).filter(g).reduce(h, 0);");
        assert_eq!(report.chains_inspected, 0);
        assert!(report.is_clean());
    }

    #[test]
    fn arguments_containing_parentheses_and_quotes_do_not_break_the_scan() {
        let report =
            audit(r#"client.prepare("k").withBody({ note: "a ) b ( c", f: g(h(1)) }).makeCall();"#);
        assert_eq!(report.chains_inspected, 1);
        assert!(report.is_clean(), "{:?}", report.issues);
    }
}

#[cfg(test)]
mod multiline_tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::embedded().expect("embedded catalog")
    }

    #[test]
    fn a_violation_is_reported_at_the_line_the_chain_starts_on() {
        let source = "const a = 1;\nconst b = 2;\nclient.prepare(\"k\").useJson().useProtobuf().makeCall();\n";
        let report = audit_source(source, &catalog(), Language::TypeScript);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].line, 3);
    }

    #[test]
    fn several_chains_in_one_file_are_audited_independently() {
        let source = r#"
            const one = await client.prepare("a").useJson().makeCall();
            const two = await client.prepare("b").useJson().useProtobuf().makeCall();
            const three = await client.prepare("c").withTimeout(5).makeCall();
        "#;
        let report = audit_source(source, &catalog(), Language::TypeScript);
        assert_eq!(report.chains_inspected, 3);
        assert_eq!(report.issues.len(), 1, "{:?}", report.issues);
        assert_eq!(report.issues[0].line, 3);
    }

    #[test]
    fn a_chain_broken_across_lines_reports_its_opening_line() {
        let source =
            "client\n  .prepare(\"k\")\n  .omitAuth()\n  .withBearerToken(t)\n  .makeCall();\n";
        let report = audit_source(source, &catalog(), Language::TypeScript);
        assert_eq!(report.chains_inspected, 1);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(
            report.issues[0].line, 2,
            "the chain begins at .prepare on line 2"
        );
    }
}
