<!-- generated-policy: writable -->

# Generated files — **not** frozen

This `generated/` directory is produced by tooling, but it is **not** the
read-only freeze policy used for flags-2-env / api-docs / interface codegen.

Files here stay writable. `chmod a-w` is **not** applied. Typical reasons:
scratch output, local overlays, music/wav, wasm build caches, k8s e2e cluster
scratch, or trees listed in `.gitignore` that generators rewrite in place.

If this tree is gitignored, this README is still committed (`git add -f` or a
`.gitignore` exception) so the exception is obvious.
