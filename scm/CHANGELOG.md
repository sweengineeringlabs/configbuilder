# Changelog

## [0.8.1] - 2026-10-27

### Changed

- Upgrade `dirs` from 5 to 7 to remove the older `dirs-sys` and `windows-sys`
  dependency line from downstream applications. The config lookup API and
  supported Rust version remain unchanged.

## [0.8.0] - 2026-08-22

### Changed

- **Breaking:** removed all remaining `swe-edge` branding — this crate is now documented and named as a standalone, ecosystem-neutral library.
  - Env vars renamed: `SWE_EDGE_CONFIG_DIR` → `CONFIGBUILDER_CONFIG_DIR`; `SWE_EDGE_FEATURE_<KEY>` / `SWE_EDGE_FEATURE_<KEY>_ON_ERROR` → `CONFIGBUILDER_FEATURE_<KEY>` / `CONFIGBUILDER_FEATURE_<KEY>_ON_ERROR`. Anyone currently setting the old names needs to update them — the crate no longer recognizes them.
  - Prose in `README.md`, `docs/README.md`, `docs/architecture.md`, and the crate-level rustdoc rewritten to drop "swe-edge services" / `swe-edge-runtime-main` framing.

### Added

- One runnable, self-verifying `cargo run --example` per config method (11 `ConfigLoaderFactory::create_*` constructors, 5 `ConfigBuilder`/`ConfigBuilderInit` fluent setters), each thoroughly documented with a "when to use this" comparison against sibling methods. Indexed in `scm/examples/docs/README.md`.

## [0.7.0] - 2026-08-21

### Added

- `ConfigBuilder::with_config_filename` — override the config filename searched for in each configured directory (was hardcoded to `application.toml` everywhere, issue #16). Defaults to `application.toml` when not called; the one-shot `create_loader_for_dir*` factory functions are unaffected and keep the default filename, consistent with how they don't expose `read_timeout` override either.

## [0.6.2] - 2026-08-21

### Fixed

- 4 integration tests (`substituter_e2e_test`, `substitution_config_builder_impl_e2e_test`, plus 2 with stray unused imports) failed to compile — missing `Loader`/`ConfigBuilder`/`BuilderFinalizer` trait imports and dead `CompositePolicy`/`PatternWhitelistPolicy`/`PrefixWhitelistPolicy` imports left over from the crate rename. `cargo test --workspace --all-features` now passes clean (99 test binaries, 598 tests).
- Reformatted 25 files with `cargo fmt` — the crate rename shortened `swe_edge_configbuilder` to `configbuilder`, shifting rustfmt's import-wrapping decisions everywhere that name appeared.

### Documentation

- Fixed `docs/architecture.md`'s stale `edge-configbuilder` title.
- Added an `## Installation` section to `README.md` now that the crate is live on crates.io.
- Updated `docs/README.md`'s dependency-pinning rationale to reflect crates.io publishing instead of git+tag.

## [0.6.1] - 2026-08-21

### Fixed

- `Cargo.toml`'s `repository` field now points at the GitHub repo's current location, `sweengineeringlabs/configbuilder` (the repo was renamed from `edge-configbuilder` to match the crate rename below).

## [0.6.0] - 2026-08-21

### Changed

- **Breaking:** renamed the crate from `swe-edge-configbuilder` to `configbuilder` on crates.io, and the Rust module from `swe_edge_configbuilder` to `configbuilder`. Update `Cargo.toml` dependency declarations and every `use swe_edge_configbuilder::...` import accordingly.

## [0.5.3] - 2026-08-16

### Fixed

- `cargo doc` now builds with zero warnings (was 7 `private_intra_doc_links` findings — doc comments pointing at `pub(crate)` inherent methods instead of their public `*Ops` trait method).
- `SubstitutionConfigBuilderImpl` — the return type of `ConfigLoaderFactory::create_config_builder_with_substitution()` — is now reachable from the crate root; it never was, even before this crate's public-surface cleanup.
- `Cargo.toml`'s `version` field now matches the crate's own release tags (was 5 releases behind).

### Documentation

- Rewrote `README.md`, `CHANGELOG.md`, and `docs/architecture.md` to match the current API and module layout; all had drifted significantly out of date. Removed two root-level `architecture.md` duplicates and a duplicate root `CHANGELOG.md`, consolidating on `scm/` as the single source of truth.

## [0.5.2] - 2026-08-16

### Fixed

- Routed the last 36 direct `api::` re-exports out of `lib.rs` and through `saf/` — `lib.rs`'s entire `pub use crate::api::` block is gone, and `saf/` is now the crate's only path to any `api::` type. Non-breaking: every `swe_edge_configbuilder::TypeName` path resolves identically.

## [0.5.1] - 2026-08-16

### Changed

- Split `api/*/types/` into `dto/` and `vo/` per domain (internal reorg, no public API change), mirrored into `core/*/vo/`.

### Fixed

- Moved 12 direct `api::` re-exports out of `saf/mod.rs` into their dedicated `_svc(_factory).rs` files, restoring the one-svc-per-trait composition structure the structural audit enforces.

## [0.5.0] - 2026-08-16

### Added

- `ValueResolver` trait and default `EnvValueResolver` implementor — `{{VAR_NAME}}` substitution values can now be sourced from something other than `std::env::var` (e.g. a secrets backend), via `ConfigLoaderFactory::create_loader_with_resolver` / `create_loader_for_dir_with_resolver` / `create_loader_xdg_with_resolver`.

## [0.4.3] - 2026-08-16

### Fixed

- Completed the SAF public-surface refactor left unfinished by the 0.4.2 compliance pass: wired 9 orphaned `saf/*_svc_factory.rs` marker files and re-exported `BuilderFinalizer`, `ConfigBuilderInit`, `FeatureRegistryOps`, `FeatureStateOps`, `FeatureRecordBuilderOps`, `FeatureSummaryOps`, `PreflightReportOps`, `PreflightIssueKindOps`, `TopologyOps`, `ConfigBuilderImpl`, `SectionLoaderImpl`, `PathValidatorImpl` through `saf/` — the missing wiring had left the test suite unable to compile.
- Fixed two `ConfigError`/`String` type-mismatch bugs in the `load_in_order!`/`preflight!` macros.
- Removed dead code: orphaned shim files, a duplicated `Topology::sort`, unused policy constructors.

## [0.4.2] - 2026-06-26

### Changed

- Structural-compliance refactor: moved impl blocks from `saf/` to `core/`, tightened core impl block visibility to `pub(crate)`, added trait impls, restricted `PreflightReport` construction to the SAF facade.
- Documented all modules; refreshed rustdoc cross-links.

## [0.4.1] - 2026-06-10

### Changed

- Bumped the `dirs` dependency 5 → 6.

## [0.4.0] - 2026-06-08

### Added

- `ConfigLoaderFactory::load_section_xdg` — one-shot XDG-resolved section load.

### Changed

- CI workflow and pre-commit `arch audit` filter fixes.

## [0.3.0] - 2026-06-06

### Changed

- Migrated to the formalized SEA package layout — the crate now lives under `scm/`.

## [0.2.0] - 2026-06-06

### Changed

- Housekeeping release ahead of the SEA layout migration.

## [0.1.0] - 2026-06-05

### Added

- Layered TOML section loader with XDG path resolution.
- `load_section`, `load_section_from`, `load_section_xdg` public functions.
- 1 MiB file size guard.
- `ConfigSection` trait for self-describing config types.
