// Human-authored TypeSpec peer authority; do not generate it from JSON Schema/OpenAPI.
// Independently maintained against json-schema/rpc-*.schema.json and
// json-schema/docs-discovery.schema.json.
// A mismatch is a release veto requiring halt and evaluate, never an overwrite.
//
// Provider-neutral lambda deployment docs are intentionally a standalone peer
// authority. Do not import them into this strict RPC root: RPC declaration-set
// admission reviews this root independently.

import "./v1.tsp";
import "./v2.tsp";
import "./telemetry.tsp";
import "./docs-discovery.tsp";
