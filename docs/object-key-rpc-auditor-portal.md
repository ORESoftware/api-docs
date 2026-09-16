# Object-key RPC auditor and integrator contract

`api-docs` treats a stable object-key operation (for example `compliance.evidence.upload`) as the semantic RPC identifier. HTTP paths, WebSocket frames, TCP framing, and NATS subjects are transport bindings of that operation.

Each route may now declare:

- `rpc_key`: stable dot-separated operation identifier;
- `authorization.mode`: `public`, `authenticated`, `service`, or `admin`;
- `authorization.roles` and `authorization.scopes`;
- optional token `audience` and user `step_up` requirement;
- `idempotency`: `none`, `optional`, or `required`;
- `data_classification`: `public`, `internal`, `confidential`, or `restricted`.

The route-map parser rejects duplicate object keys, invalid object-key syntax, impossible public privilege requirements, service/user step-up confusion, and required idempotency on non-mutating operations.

## Security boundary

This metadata documents and generates authorization requirements; it does not authenticate a caller. Runtime middleware must authenticate first and construct the trusted request context from verified claims. Tenant ID, actor ID, roles, scopes, session ID, and authentication strength are server-owned context and must never be accepted from caller-controlled RPC metadata or application headers.

## Auditor view

The static documentation site should render a matrix with one row per `rpc_key` and columns for owning service, transport binding, authorization mode, roles, scopes, audience, step-up, idempotency, data classification, request/response/error schemas, and source/provenance digest. This provides living evidence of the declared logical-access boundary while keeping the executable authorization check in the service.

## Integrator view

Client developers should see the same operation identifiers, generated request/response types, examples, error codes, supported transports, retry/idempotency contract, and contract digest. The docs must never display secret values or internal credential references.

`docs/examples/canonical-compliance.route-map.json` is the initial compliance-platform example. It intentionally sits outside the checked-in generated-route inventory until the exact generator can emit every required language projection in the same change.
