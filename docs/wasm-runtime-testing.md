# WASM runtime contract tests

The Rust WASM compilation gate checks target compatibility. These tests also run
real generated JavaScript glue and WebAssembly, including JavaScript-only input
values (`undefined`, `NaN`, and infinity), precision changes, failed updates,
full-document formatting, and answer/aggregate semantics.

Install `wasm-bindgen-cli` matching the `wasm-bindgen` version in `Cargo.lock`
(currently `0.2.126`) from crates.io, then run from the workspace root:

```sh
cargo install wasm-bindgen-cli --version 0.2.126 --locked
cargo build --locked -p numr-core --target wasm32-unknown-unknown \
  --no-default-features --features wasm
out=$(mktemp -d)
wasm-bindgen --target nodejs --out-dir "$out/wasm-node" \
  target/wasm32-unknown-unknown/debug/numr_core.wasm
node scripts/test-wasm-runtime.mjs "$out/wasm-node/numr_core.js"
```

The Node test exits nonzero if any assertion fails. For a real browser run,
reuse the same build and temporary output directory:

```sh
wasm-bindgen --target web --out-dir "$out/wasm-web" \
  target/wasm32-unknown-unknown/debug/numr_core.wasm
cp scripts/test-wasm-runtime.{mjs,html} "$out/"
python3 -m http.server 8765 --directory "$out"
```

Open `http://localhost:8765/test-wasm-runtime.html` in a browser. The page must
show `PASS` and the assertion count, rather than `RUNNING` or `FAIL`. This page
can also run in headless Chromium with `--dump-dom --virtual-time-budget=10000`;
check the rendered `#status` element because browser exit status alone does not
indicate assertion success.

This validates the core boundary without needing the separate `numr-web`
checkout and does not change its generated deployable package. The web app's
own build, browser tests, and generated-artifact synchronization remain separate
release gates.
