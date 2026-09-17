# Generated RPC language layout

RPC operation files are projected from the canonical dotted `operation_key`. The wire key remains authoritative; filesystem paths are generated namespace projections.

## Five generated roots

There is exactly one operation-generated root per supported language:

- `rust/generated/`
- `go/generated/`
- `dart/lib/generated/`
- `typescript/generated/`
- `gleam/src/generated/`

Generic transport/envelope/admission runtime code lives outside these roots under the language `runtime/` tree. Regular/admin are runtime variants and operation namespaces, not duplicate language trees.

Examples:

- `sonus_auris.version.get_version` → `<language-generated-root>/version/get_version.<ext>`
- `sonus_auris.admin.version.get_version` → `<language-generated-root>/admin/version/get_version.<ext>`
- `canonical_cloud.version.get_version` → `<language-generated-root>/version/get_version.<ext>`

The first dotted segment is the service/root identity and is not repeated as a filesystem directory. Remaining intermediate segments are namespace folders. The final segment is the snake_case operation filename.

## Generated-tree contract

Each language `generated/` root contains `README.md` and `AGENTS.md` directly inside it. Every generated source file begins with a generated/do-not-edit header.

`ores-stack sync` temporarily makes an existing generated tree writable, materializes deterministic output, then removes write bits recursively. Git does not retain ordinary read-only permission bits, so checkout state alone is not proof of the chmod policy; the generator reapplies it after materialization.

Generated operation files must never own generic transport code. Conversely, runtime files must not define application-specific named calls such as `getVersion`, `GetVersion`, or `get_version`.
