/**
 * Exercise generated wasm-bindgen glue in Node or a browser, not native Rust.
 * Node: node scripts/test-wasm-runtime.mjs /absolute/path/to/nodejs/numr_core.js
 * Browser: import { testWasmRuntime } from this module after initializing web glue.
 */
export function testWasmRuntime(WasmEngine) {
  let checks = 0;
  const equal = (actual, expected, label) => {
    checks++;
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
    }
  };
  const engine = new WasmEngine();
  const document = content => JSON.parse(engine.eval_document_full(content));
  const result = content => document(content).results.at(-1).result;
  try {
    const original = document('152 / 150\n$12.34567');
    for (const [places, expected] of [[0, '1'], [4, '1.0133'], [28, '1.0133333333333333333333333333']]) {
      engine.set_decimal_places(places);
      equal(result('152 / 150'), expected, `precision ${places}`);
    }
    engine.set_decimal_places(undefined);
    equal(document('152 / 150\n$12.34567'), original, 'undefined restores defaults');
    engine.set_decimal_places(4);
    const expected = document('152 / 150\n$12.34567');
    for (const invalid of [-1, 1.5, 29, 4294967296, NaN, Infinity, -Infinity]) {
      let error;
      try { engine.set_decimal_places(invalid); } catch (caught) { error = caught; }
      equal(String(error), 'decimal places must be between 0 and 28', `reject ${invalid}`);
      equal(document('152 / 150\n$12.34567'), expected, `preserve setting after ${invalid}`);
    }
    engine.set_decimal_places(0);
    equal(result('fraction = 152 / 150\nfraction * 150'), '152', 'display rounding preserves arithmetic');
    engine.set_decimal_places(4);
    equal(document('price = $12.34567').totals, ['$12.3457'], 'currency totals use precision');
    equal(document('price = $12.34567').variable_names, 'price', 'full document includes variables');
    equal(result('third = 1 / 3\nthird * 3'), '1', 'full precision variables');
    for (const input of ['200 # ANS _ ans total', '200 // ANS _ ans total']) {
      equal(result(`100\n${input}\ntotal`), '300', `comment references: ${input}`);
    }
    for (const input of ['total # subtotal', 'total // ANS', '(total)']) {
      equal(result(`100\n${input}\ntotal`), '100', `display-only aggregate: ${input}`);
    }
    equal(result('100\nresult = sum(ANS, 20)\ntotal'), '120', 'nested ANS consumes source');
    equal(result('100\nANS / 0\ntotal'), '100', 'failed ANS leaves source');
    equal(result('100\ntotal + 0\ntotal'), '200', 'total expression contributes');
    equal(document('1').results[0].is_error, false, 'document replacement resets prior errors');
    return { passed: checks };
  } finally {
    engine.free();
  }
}

if (typeof process !== 'undefined' && process.argv?.[1]) {
  const { pathToFileURL } = await import('node:url');
  if (import.meta.url === pathToFileURL(process.argv[1]).href) {
    if (!process.argv[2]) throw new Error('Pass the generated nodejs numr_core.js path');
    const { resolve } = await import('node:path');
    const { WasmEngine } = await import(pathToFileURL(resolve(process.argv[2])).href);
    console.log(JSON.stringify(testWasmRuntime(WasmEngine)));
  }
}
