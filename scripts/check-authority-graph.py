#!/usr/bin/env python3
"""Validate the peer contract-authority graph.

This gate is deliberately independent of any emitter. It prevents a generator,
ORM, client, or compatibility ledger from silently becoming an authoring
authority, and it makes every SQL/type/ORM discrepancy a release veto until a
human evaluates and records the delta.
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any, Iterable


EXPECTED_AUTHORITIES = {"typespec", "json-schema-openapi"}
EXPECTED_EDGES = {
    ("typespec", "typespec-sql", "projection"),
    ("typespec", "protobuf", "projection"),
    ("typespec", "grpc", "projection"),
    ("protobuf", "typespec-wire-types", "projection"),
    ("grpc", "typespec-wire-clients", "projection"),
    ("json-schema-openapi", "jsonschema-interfaces", "projection"),
    ("json-schema-openapi", "jsonschema-client-types", "projection"),
    ("json-schema-openapi", "jsonschema-sql", "projection"),
    ("json-schema-openapi", "jsonschema-write-clients", "projection"),
    ("typespec-sql", "sql-parity-gate", "verification"),
    ("jsonschema-sql", "sql-parity-gate", "verification"),
    ("sql-parity-gate", "normalized-sql", "admission"),
    ("typespec-wire-types", "type-parity-gate", "verification"),
    ("jsonschema-client-types", "type-parity-gate", "verification"),
    ("type-parity-gate", "normalized-types", "admission"),
    ("normalized-sql", "diesel-models", "projection"),
    ("normalized-sql", "seaorm-models", "projection"),
}
EXPECTED_COMPARISONS = {
    frozenset(("typespec-sql", "jsonschema-sql")): "postgres-semantic-ir",
    frozenset(("typespec-wire-types", "jsonschema-client-types")): "language-type-ir",
    frozenset(("diesel-models", "seaorm-models")): "orm-model-ir",
    frozenset(("diesel-models", "normalized-sql")): "orm-vs-postgres-ir",
    frozenset(("seaorm-models", "normalized-sql")): "orm-vs-postgres-ir",
}
REQUIRED_BLOCKS = {"generation", "commit", "merge", "release", "deployment"}
GENERATIVE_EDGE_KINDS = {"projection", "admission"}


def _objects(value: Any, field: str, errors: list[str]) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        errors.append(f"{field} must be an array")
        return []
    out: list[dict[str, Any]] = []
    for index, item in enumerate(value):
        if not isinstance(item, dict):
            errors.append(f"{field}[{index}] must be an object")
            continue
        out.append(item)
    return out


def _unique_ids(items: Iterable[dict[str, Any]], field: str, errors: list[str]) -> set[str]:
    seen: set[str] = set()
    for index, item in enumerate(items):
        value = item.get("id")
        if not isinstance(value, str) or not value:
            errors.append(f"{field}[{index}].id must be a non-empty string")
            continue
        if value in seen:
            errors.append(f"duplicate {field} id: {value}")
        seen.add(value)
    return seen


def _find_cycle(node_ids: set[str], edges: list[dict[str, Any]]) -> list[str] | None:
    graph: dict[str, list[str]] = defaultdict(list)
    for edge in edges:
        if edge.get("kind") in GENERATIVE_EDGE_KINDS:
            source, target = edge.get("from"), edge.get("to")
            if isinstance(source, str) and isinstance(target, str):
                graph[source].append(target)

    state: dict[str, int] = {node: 0 for node in node_ids}
    stack: list[str] = []

    def visit(node: str) -> list[str] | None:
        state[node] = 1
        stack.append(node)
        for target in graph.get(node, []):
            if target not in state:
                continue
            if state[target] == 0:
                cycle = visit(target)
                if cycle:
                    return cycle
            elif state[target] == 1:
                start = stack.index(target)
                return stack[start:] + [target]
        stack.pop()
        state[node] = 2
        return None

    for node in sorted(node_ids):
        if state[node] == 0:
            cycle = visit(node)
            if cycle:
                return cycle
    return None


def validate(document: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(document, dict):
        return ["root must be an object"]

    if document.get("schema_version") != "1.0.0":
        errors.append("schema_version must be 1.0.0")

    authorities = _objects(document.get("authorities"), "authorities", errors)
    authority_ids = _unique_ids(authorities, "authority", errors)
    if authority_ids != EXPECTED_AUTHORITIES:
        errors.append(
            "authorities must be exactly the two peer lanes: "
            f"{sorted(EXPECTED_AUTHORITIES)}; got {sorted(authority_ids)}"
        )
    for authority in authorities:
        authority_id = authority.get("id", "<unknown>")
        if authority.get("kind") != "human-authored":
            errors.append(f"{authority_id}: authority kind must be human-authored")
        if authority.get("top_level") is not True:
            errors.append(f"{authority_id}: authority must be top_level=true")
        sources = authority.get("sources")
        if not isinstance(sources, list) or not sources or not all(
            isinstance(source, str) and source for source in sources
        ):
            errors.append(f"{authority_id}: authority sources must be non-empty strings")

    nodes = _objects(document.get("nodes"), "nodes", errors)
    node_ids = _unique_ids(nodes, "node", errors)
    missing_authority_nodes = EXPECTED_AUTHORITIES - node_ids
    if missing_authority_nodes:
        errors.append(f"missing authority nodes: {sorted(missing_authority_nodes)}")

    node_by_id = {
        node["id"]: node
        for node in nodes
        if isinstance(node.get("id"), str) and node.get("id")
    }
    for authority_id in EXPECTED_AUTHORITIES & node_ids:
        if node_by_id[authority_id].get("kind") != "authority":
            errors.append(f"{authority_id}: corresponding node kind must be authority")

    edges = _objects(document.get("edges"), "edges", errors)
    edge_tuples: set[tuple[str, str, str]] = set()
    for index, edge in enumerate(edges):
        source, target, kind = edge.get("from"), edge.get("to"), edge.get("kind")
        if not all(isinstance(value, str) and value for value in (source, target, kind)):
            errors.append(f"edges[{index}] from/to/kind must be non-empty strings")
            continue
        if source not in node_ids:
            errors.append(f"edges[{index}] references unknown source node {source}")
        if target not in node_ids:
            errors.append(f"edges[{index}] references unknown target node {target}")
        edge_tuples.add((source, target, kind))

        if target in EXPECTED_AUTHORITIES:
            errors.append(
                f"{source} -> {target}: generated or downstream nodes may not feed an authority"
            )

    for missing in sorted(EXPECTED_EDGES - edge_tuples):
        errors.append(f"missing required edge: {missing[0]} -> {missing[1]} ({missing[2]})")

    cycle = _find_cycle(node_ids, edges)
    if cycle:
        errors.append(f"generative graph contains a cycle: {' -> '.join(cycle)}")

    comparisons = _objects(document.get("comparisons"), "comparisons", errors)
    comparison_map: dict[frozenset[str], dict[str, Any]] = {}
    for index, comparison in enumerate(comparisons):
        left, right = comparison.get("left"), comparison.get("right")
        if not isinstance(left, str) or not isinstance(right, str) or left == right:
            errors.append(f"comparisons[{index}] needs two distinct node ids")
            continue
        if left not in node_ids or right not in node_ids:
            errors.append(f"comparisons[{index}] references an unknown node")
        key = frozenset((left, right))
        if key in comparison_map:
            errors.append(f"duplicate comparison: {sorted(key)}")
        comparison_map[key] = comparison
        if comparison.get("on_discrepancy") != "pause-and-evaluate":
            errors.append(
                f"{left} vs {right}: on_discrepancy must be pause-and-evaluate"
            )
        evidence = comparison.get("evidence")
        if not isinstance(evidence, list) or not evidence:
            errors.append(f"{left} vs {right}: evidence must be a non-empty array")

    for pair, normalizer in EXPECTED_COMPARISONS.items():
        comparison = comparison_map.get(pair)
        if comparison is None:
            errors.append(f"missing required comparison: {sorted(pair)}")
        elif comparison.get("normalizer") != normalizer:
            errors.append(
                f"{sorted(pair)} must use normalizer {normalizer!r}; "
                f"got {comparison.get('normalizer')!r}"
            )

    policy = document.get("discrepancy_policy")
    if not isinstance(policy, dict):
        errors.append("discrepancy_policy must be an object")
    else:
        if policy.get("mode") != "pause-and-evaluate":
            errors.append("discrepancy_policy.mode must be pause-and-evaluate")
        if policy.get("no_automatic_winner") is not True:
            errors.append("discrepancy_policy.no_automatic_winner must be true")
        blocks = policy.get("blocks")
        block_set = set(blocks) if isinstance(blocks, list) else set()
        missing_blocks = REQUIRED_BLOCKS - block_set
        if missing_blocks:
            errors.append(
                f"discrepancy_policy.blocks missing: {sorted(missing_blocks)}"
            )
        delta_file = policy.get("expected_delta_file")
        if not isinstance(delta_file, str) or not delta_file:
            errors.append("discrepancy_policy.expected_delta_file must be set")
        required_evidence = policy.get("required_evidence")
        if not isinstance(required_evidence, list) or len(required_evidence) < 3:
            errors.append(
                "discrepancy_policy.required_evidence must contain at least three items"
            )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "graph",
        nargs="?",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "idl" / "authority-graph.json",
    )
    args = parser.parse_args()

    try:
        document = json.loads(args.graph.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"authority graph could not be read: {error}", file=sys.stderr)
        return 2

    errors = validate(document)
    if errors:
        print("authority graph rejected:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(
        "authority graph accepted: two peer authorities, fail-closed SQL/type "
        "parity, and Diesel/SeaORM convergence are enforced"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
