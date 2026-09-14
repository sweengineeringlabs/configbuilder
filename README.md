# configbuilder

> **TLDR:** Standalone TOML section loader for any Rust application — XDG-aware layered resolution, `{{VAR_NAME}}` substitution with pluggable name policies and value sources, and preflight validation. No runtime dependency.

Standalone, runtime-independent TOML section loader for any Rust application. Provides
XDG-aware, layered config section loading for any `T: DeserializeOwned + Default`. Library
crates can depend on this crate directly without pulling in any application runtime.

The crate lives under [`scm/`](scm/) (the formalized SEA package layout). Full usage
examples, the feature list, and the API surface are documented there — this page is
intentionally a pointer, not a duplicate, so it can't drift out of sync the way it
previously did.

## Documentation

| Document | Description |
|----------|-------------|
| [README](scm/README.md) | Usage examples — basic loading, substitution, preflight, builder pattern |
| [Overview](scm/docs/README.md) | WHAT + WHY — capabilities and design rationale |
| [Architecture](scm/docs/architecture.md) | SEA module layout, sequence/data-flow diagrams, key contracts |
| [Changelog](scm/CHANGELOG.md) | Version history |
| [Rustdoc](https://docs.rs/configbuilder) | Full API reference |
