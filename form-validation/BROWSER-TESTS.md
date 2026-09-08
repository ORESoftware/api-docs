# Browser execution and submission regressions (DEN-3045)

`tests/browser.rs` compiles the existing native conformance and boundary test
bodies into WASM and calls them through `wasm-bindgen-test` in headless Chrome.
It does not reimplement the validator in JavaScript. The browser configuration
is explicit, so a Node-only fallback is not accepted as browser evidence.

The production crate retains its native/WASM dependency boundary; the browser
harness is a target-specific development dependency. The runner is pinned to
0.2.128, matching wasm-bindgen-test 0.3.78. CI records browser/driver versions,
executes the existing 85-fixture corpus and additional mailbox/Unicode/calendar/
safe-integer and stale-form-state checks, and retains the exact-head test log.

The submission regression deliberately changes the value after a successful
edit without emitting another edit event. `submit` must revalidate that value,
reject invalid/absent input, and clear old errors after correction. This is
relevant to autofill, programmatic edits and sync conflict resolution; it does
not authorize credentials or other secrets to be synchronized.

During initial dependency resolution, CI retains a lockfile/formatting candidate
but fails if that candidate differs from the committed source. Existing native,
Dart/JavaScript, Flutter-widget and renderer workflows remain required evidence.

This extends the README's earlier WASM compilation evidence to real execution
of the **core** in Chrome. It is not Leptos/Dioxus hydration, application input
wiring, authentication, database, mobile or native-window E2E evidence.

Reference: https://wasm-bindgen.github.io/wasm-bindgen/wasm-bindgen-test/browsers.html
