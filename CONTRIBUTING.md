# Contributing to Kamish

Thank you for your interest in contributing to Kamish!

## Code Quality Standards

All contributions must strictly adhere to the following quality gates:

1. **Rust Code Quality**:
   - Clean formatting: `cargo fmt --check`
   - Zero warnings: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   - Complete tests: `cargo test --workspace`
   - Verification probes: `cd early_research/probes && cargo test`
2. **Frontend & SDK**:
   - `cd frontend && npm test && npm run build`
   - `cd packages/kamish-client && npm test && npm run build`
3. **Packaging & Clean-Room Test**:
   - `python3 scripts/test_clean_room.py`

## Development Workflow

1. Fork and clone the repository.
2. Initialize local dependencies and build the unified binary:
   ```bash
   cargo build -p kamish
   ```
3. Run diagnostic checks:
   ```bash
   cargo run -p kamish -- doctor
   ```
4. Commit messages should be descriptive and reference corresponding requirement IDs (e.g., `FR-xx`, `NFR-xx`, `INV-xx`).
