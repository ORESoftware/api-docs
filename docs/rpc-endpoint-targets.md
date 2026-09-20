# RPC endpoint targets

Generated RPC clients may execute the same admitted operation inventory through either the standalone API-server HTTP ingress or the Lambda HTTP ingress.

The canonical selector is a closed value: `default`, `standalone`, or `lambda`. Compatibility booleans may normalize to this selector, but contradictory booleans fail closed.

The operation key, `/v1/rpc` path, request envelope, receipt validation, authentication metadata, audience and authorization semantics are unchanged. Only the configured origin changes, for example `https://api.example.com/v1/rpc` versus `https://lambda.example.com/v1/rpc`.

`lambda` means the HTTP-ingress Lambda deployment. It never selects a provider direct-invoke carrier.

A client configured without the requested endpoint fails instead of silently falling back. Endpoint target is part of local execution/cache/dedupe identity and is reported separately from the server receipt `transport` field.
