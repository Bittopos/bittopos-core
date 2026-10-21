# bittopos-core

Canonical shared Rust crate for Bittopos.

This crate owns the semantic domain model, profile/project/resource operations, workspace lifecycle, and SQLite persistence primitives. The CLI and server should depend on this crate instead of copying domain logic.

## Dependency from another repository

Pin a reviewed immutable commit in each consumer's `Cargo.toml`:

```toml
[dependencies]
bittopos-core = { git = "https://github.com/jasperalsain/bittopos-core", rev = "<reviewed-commit-sha>" }
```

Do not use an unpinned branch in production. Commit the application's `Cargo.lock` to make builds reproducible. Update the pinned SHA through a reviewed PR when the core changes.

## Checks

```bash
cargo fmt --all -- --check
cargo build
cargo test
```

License: Apache-2.0. See `LICENSE`.
