# Web surface routing

`*-web-server.rs` repositories classify browser requests into disjoint surfaces before any filesystem, object-store, page-renderer, or docs-renderer access occurs.

## Canonical HTTP namespaces

- `GET|HEAD /static/**` is the static-asset surface.
- `GET|HEAD /_/docs` and `GET|HEAD /_/docs/**` are the generated documentation surface.
- other admitted `GET|HEAD` paths are the page surface backed by `src/pages/**/page.rs`.

A miss is terminal inside the selected surface. `/static/missing.png` is a static 404 and must never fall through to page routing. Likewise a docs miss must never fall through to pages.

`src/pages/static/**` and `src/pages/_/**` are reserved and must fail source admission. This makes route class selection a pure function of the normalized HTTP path rather than request-time filesystem probing.

## Source layout

```text
src/
  main.rs
  pages/**
    page.rs
    gen.rs        # optional authored static parameter generation
    lambda.rs     # generated where page Lambda deployment is enabled
  mounts/
    static/
      route.rs
      lambda.rs   # generated one-per-surface
    docs/
      route.rs
      lambda.rs   # generated one-per-surface
assets/
  static/
    public/**
    private/**
```

`src/main.rs` composes already-classified surfaces. It is not a route-discovery authority.

The static and docs mounts are not page trees. Static assets may be public or admission-protected, but both are selected through the `/static/**` surface. Generated docs are independently produced artifacts mounted below `/_/docs/**` and do not participate in page layout/rendering semantics.

## Build/runtime rule

Route manifests are compiled before serving. Runtime classification must not `stat`, walk, or probe multiple source trees to decide whether a request is a static file, docs resource, or page. Production implementations may resolve a selected static/docs manifest entry to packaged bytes, R2/S3/object storage, or another immutable artifact location.

The static surface and docs surface each have one provider-neutral Lambda unit. Page Lambda topology remains independently configurable (for example one Lambda per page). This preserves standalone/Lambda parity without creating one Lambda per static file.

Framework-owned `/_/**` remains reserved. Dev-only transports such as browser reload/WebSocket infrastructure belong to `ores-stack dev`, not product source authority.
