// `npm test` names its files one by one, so a test file that is not on the list
// is never run and nothing says so. fluent-identity.test.js was written, passed
// when run by hand, and was skipped by CI until this check existed.

import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";

test("every test file in src/ is run by `npm test`", () => {
  const root = new URL("../", import.meta.url);
  const script = JSON.parse(readFileSync(new URL("package.json", root), "utf8")).scripts.test;
  const listed = new Set(script.split(/\s+/).filter((word) => word.endsWith(".test.js")));
  const present = readdirSync(new URL("src/", root))
    .filter((name) => name.endsWith(".test.js"))
    .map((name) => `src/${name}`);
  assert.deepEqual(
    present.filter((file) => !listed.has(file)),
    [],
    "test files that `npm test` never runs",
  );
  assert.deepEqual(
    [...listed].filter((file) => !present.includes(file)),
    [],
    "`npm test` names files that do not exist",
  );
});
