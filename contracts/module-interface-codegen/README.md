# Module interface codegen contract

`module-interface-codegen.tsp` and `module-interface-codegen.schema.json` are independently authored peer authorities for requests to the shared module-interface projection engine.

Neither file is generated from the other. Rust deserialization/rendering is an implementation of the parity-approved contract, not a replacement authority.

The retained `fixtures/scintilla-worker.json` request is validated by the Draft 2020-12 schema, deserialized through the Rust contract types, and rendered through the same `render_module_interface_matrix` API used by consumers.

Runtime profiles are admission boundaries. In particular, the `beam_scale` profile intentionally rejects non-BEAM guest projections even though the shared renderer knows how to render those languages for other profiles.

Generated language sources are read-only projections. Domain input/output payload contracts remain owned by the consuming service's TypeSpec + JSON Schema/OpenAPI authorities; this contract only selects and renders the language-level module boundary.
