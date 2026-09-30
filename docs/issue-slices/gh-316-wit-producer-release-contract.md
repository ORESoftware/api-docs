# api-docs WIT producer release admission

Driver: `ORESoftware/api-docs#316`

This document captures one bounded review contract for an independently mergeable slice of the driver issue; it does not claim full implementation.

## Invariants

- Every WIT producer runs through the shared ores-wit toolchain at an immutable reviewed revision.
- TJSV evidence binds authored contract authority to the exact generated WIT projection.
- Generated WIT is downstream evidence and cannot become a third authored semantic authority.
- Release admission requires no-diff regeneration plus tool/version/source provenance.

## Verification

- Verify the exact PR head with the repository's normal contract/test gates.
- Include fail-closed negative cases for malformed or unsupported input.
- Bind generated evidence to immutable producer/tool/source identity.
- Treat missing, skipped, or zero-step CI as missing evidence.

## Non-goals

This slice does not add credentials, bypass review, or change authored contract authority without the driver issue's explicit implementation work.
