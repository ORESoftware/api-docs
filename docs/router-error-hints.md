# Router error hint contract

Generated Axum routers should answer framework-generated HTTP errors with JSON route/path hints derived only from the already-compiled route inventory. Request handling must never probe the filesystem to discover suggestions.

Canonical shape:

```json
{
  "status": 404,
  "code": "not_found",
  "message": "route not found",
  "suggestions": [
    { "path": "/users/{id}", "methods": ["GET", "HEAD"] }
  ]
}
```

Requirements:

- 404 and 405 use the in-memory router manifest and deterministic ranking;
- 405 suggestions prefer the same path and enumerate admitted methods;
- framework errors with status `>= 400` may include relevant safe route hints;
- successful responses never include error suggestions;
- public responses must not disclose admin/private/internal-only routes;
- `/_/admin/**` and `/__ores/**` are never suggested by a public router;
- a miss is terminal within the selected routing surface and never causes cross-surface filesystem probing.
