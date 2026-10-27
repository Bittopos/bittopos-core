# bittopos-core

Canonical Rust domain and storage crate for Bittopos. The application API is kept in this repository, but separated from the core under `src/api/`.

## Responsibility boundary

- **Core:** domain types and invariants, semantic resolution, resource/workspace storage and serialization, and persistence primitives.
- **API (`bittopos_core::api`):** operations that coordinate core facilities for interface consumers, including application use cases, navigation context, and semantic-mask workflows.
- **Interfaces:** user interaction, interface-specific state and orchestration, and process lifecycle. The core and API do not manage operating-system processes.

The API is a boundary over the core, not part of the domain model. Domain semantics should have one canonical implementation in the core; the API delegates to those rules rather than duplicating them.

## Current extraction status

This change establishes the explicit API module and moves application orchestration, navigation, semantic-mask workflows, and read-only model queries behind it. The existing instance lifecycle and resource/reference/relation creation paths remain coupled to core storage modules and need a further focused extraction before this boundary can be considered complete. This PR must therefore be reviewed as an architectural step, not as proof that all core responsibilities have already been minimized.

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
