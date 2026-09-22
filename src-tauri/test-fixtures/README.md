# Pipeline test fixtures

Committed fixtures for pipeline E2E and contract regression tests.

## Strips

| File | Description |
|------|-------------|
| `strips/walk-4-horizontal.png` | 4-frame horizontal walk strip (32×32 cells, red foot pixels) |

Refresh the walk strip with:

```bash
cargo test materialize_walk_strip_fixture --manifest-path src-tauri/Cargo.toml -- --ignored
```

## Running regression tests

Full lib suite (local gate):

```bash
cargo test --lib --manifest-path src-tauri/Cargo.toml
bun run check
```

Pipeline E2E + fixture contract regression only:

```bash
cargo test --lib --manifest-path src-tauri/Cargo.toml -- pipeline_e2e fixture_regression
```

CI runs `frontend`, `rust-lib`, and `rust-pipeline-e2e` on Linux and macOS.

## Production score gate

`production_score_fixture_gate` (in `fixture_regression_tests.rs`) normalizes the walk strip fixture and asserts `overallScore >= 60` (`PRODUCTION_SCORE_CI_MIN`). This runs inside the existing `rust-pipeline-e2e` job — no separate workflow job.
