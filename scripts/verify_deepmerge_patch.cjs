// The Jest-only deepmerge dependency has no published fix for CVE-2026-93753.
// Verify every patched entry point before accepting its audit exception.
const assert = require("node:assert/strict");
const path = require("node:path");
const { createRequire } = require("node:module");
const sdk = createRequire(
  path.resolve(__dirname, "../sdk/typescript/package.json"),
);
const jest = createRequire(sdk.resolve("jest-config"));
const root = path.dirname(jest.resolve("deepmerge/package.json"));
assert.equal(jest("deepmerge/package.json").version, "4.3.1");
for (const entry of ["dist/cjs.js", "dist/umd.js"]) {
  const merge = require(path.join(root, entry));
  for (const key of ["__proto__", "constructor", "prototype"]) {
    for (const prototype of [null, Object.prototype]) {
      const poisoned = Object.create(prototype);
      Object.defineProperty(poisoned, key, {
        enumerable: true,
        value: { polluted: true },
      });
      for (const result of [merge(poisoned, {}), merge({}, poisoned)]) {
        assert.equal(Object.getPrototypeOf(result), Object.prototype);
        assert.equal(Object.hasOwn(result, key), false);
        assert.equal(result.polluted, undefined);
      }
      const nested = merge({}, { nested: poisoned, array: [poisoned] });
      for (const result of [nested.nested, nested.array[0]]) {
        assert.equal(Object.getPrototypeOf(result), Object.prototype);
        assert.equal(Object.hasOwn(result, key), false);
        assert.equal(result.polluted, undefined);
      }
    }
  }
  assert.deepEqual(merge({ a: [1], b: { x: 1 } }, { a: [2], b: { y: 2 } }), {
    a: [1, 2],
    b: { x: 1, y: 2 },
  });
  assert.equal({}.polluted, undefined);
}
console.log(
  "deepmerge: all entry points reject prototype poisoning; ordinary merges pass",
);
