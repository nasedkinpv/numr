# Project Notes

## Repository Layout

- `numr-core`: Pure parser/evaluator, values, units/currencies, document API, catalogs, and optional native rate fetching. `Engine::new()` performs no filesystem or network I/O.
- `numr-editor`: Shared syntax highlighting and small UTF-8 text primitives. It does not own a text buffer.
- `numr-tui`: Event-driven Ratatui frontend with Vim and Standard modes, cached render state, atomic persistence, and a persistent background rate worker.
- `numr-cli`: Command-line, REPL, and newline-delimited JSON-RPC 2.0 interfaces.
- `numr-web`: Separate Git repository checked out at `numr/numr-web` for a compatible build. It is not a submodule.

The workspace MSRV is Rust 1.88. Keep native release builds runtime-oriented; the web build entrypoints override optimization to `z` for WASM size.

## Core Interfaces

- Use `Engine::evaluate_document` when replacing a full document and `Engine::append_lines` for stateful incremental input. `DocumentResult` is the shared document result shape.
- Load/save the native rate cache explicitly with `load_rates_from_cache` and `save_rates_to_cache`; apply fetched/browser rates through `apply_raw_rates`/`apply_rates`.
- Reuse `catalog::{BUILTIN_FUNCTIONS, KEYWORDS, MATH_CONSTANTS, ANSWER_ALIASES, currency_catalog}` instead of duplicating language or currency metadata.
- Preserve typed `ParseError`, `EvalError`, and `RateError` boundaries. Do not reintroduce unchecked Decimal arithmetic in user-controlled evaluation paths.
- Default parser limits are 16 KiB input, 256 operations, and 128 nesting levels. Adapters may add tighter transport limits but must not bypass core limits.

## Release Checklist

For 0.7.0, use `0.7.0`/`v0.7.0` in the commands below. For later releases, replace `X.Y.Z` consistently.

1. Bump the workspace version in `Cargo.toml`. In the separate web repository, bump `package.json` and `electrobun.config.ts`; regenerated WASM package manifests must end at the same version.
2. Refresh the workspace lockfile after the version bump:

   ```bash
   cargo build --workspace
   ```

3. Run the locked Rust gates from the workspace root:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
   cargo test --locked --workspace --all-features
   cargo build --locked --workspace
   cargo check --locked -p numr-core --target wasm32-unknown-unknown --no-default-features --features wasm
   cargo check --locked -p numr-editor --target wasm32-unknown-unknown --no-default-features --features wasm
   cargo check --locked -p numr-core --target wasm32-unknown-unknown --all-features
   ```

4. Regenerate and verify the web artifacts:

   ```bash
   cd numr-web
   npm run build
   npm run check
   cd ..
   ```

   `npm run build` cleans and rebuilds both WASM packages, stamps asset versions, and verifies version synchronization after generation, so it also works immediately after a version bump. Review and commit changes under `numr-web/pkg/`; the npm package intentionally includes both WASM packages.

5. Verify the native release build used by packaging:

   ```bash
   cargo build --release --locked --workspace
   ```

6. Commit `Cargo.toml` and `Cargo.lock` together in the main repository. Commit the compatible source and generated WASM artifacts in the separate `numr-web` repository.
7. Push in this strict order so Web CI never checks out the previous core release:

   1. push the main repository branch without a tag;
   2. push the compatible `numr-web` branch;
   3. wait for both repositories' CI runs to pass;
   4. tag `numr-web` with the same version;
   5. only then create and push the main release tag.

   `numr-web/scripts/check-version-sync.mjs` must continue comparing the checked-out workspace version with Web, desktop, and generated WASM package versions.

8. Create and push the main repository tag:

   ```bash
   git tag vX.Y.Z
   git push origin master --tags
   ```

9. Monitor `.github/workflows/release.yml`. The tag-triggered workflow owns the release: it validates tag/version alignment, runs Rust/WASM gates, creates the GitHub Release, uploads locked binary builds, and then updates AUR and the Homebrew tap for stable tags.

`gh release create` is recovery-only. Use it only if the automated release-creation job cannot be rerun and the GitHub Release is still missing.

## Release Notes

- Homebrew builds with `--locked`; never leave a version bump without its updated `Cargo.lock`.
- Do not precompute AUR/Homebrew checksums. The release workflow derives them from uploaded artifacts, publishes AUR metadata, and pushes the updated formula to `nasedkinpv/homebrew-tap`.
- The root release workflow checks WASM compilation but does not publish the separate web repository. Keep its compatible commit and generated `pkg/` artifacts synchronized explicitly.

## WASM and Web Development

Both `numr-core` and `numr-editor` compile with `--no-default-features --features wasm`:

```bash
cd numr-web
npm run check
npm run build
npm run serve
```

The web CI checks out `nasedkinpv/numr` into `numr/`, checks out `nasedkinpv/numr-web` into `numr/numr-web`, runs JavaScript contract tests, rebuilds both WASM packages with `--locked`, and rejects uncommitted generated output.

## Language Semantics

- Lines starting with operators (`+ 10`, `* 2`) continue from the previous successful result.
- `_`, `ANS`, and `ans` reference the previous successful result.
- Comments start with `#` or `//`.
- Aggregate lines such as bare `total` are display-only and do not feed later totals.
