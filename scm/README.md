# configbuilder

Standalone, runtime-independent TOML section loader for any Rust application.

Provides XDG-aware, layered config section loading for any `T: DeserializeOwned + Default`.
It has no dependency on any application runtime, so library crates can depend on it directly.

All consumer-facing behavior is reached through `ConfigLoaderFactory` — the crate's SAF
(Service Access Facade). You never construct a loader, builder, or policy type directly.

## Installation

```toml
[dependencies]
configbuilder = "0.9"
```

## Features

- **Layered config resolution** — merges config from multiple directories, later sources win
- **XDG Base Directory support** — automatic path resolution via `$XDG_CONFIG_HOME`, `$XDG_CONFIG_DIRS`, `$CONFIGBUILDER_CONFIG_DIR`
- **Overridable config filename** — searches for `application.toml` by default; override with `with_config_filename`
- **Dotted key paths** — load nested sections with `"outer.inner"` syntax
- **Optional feature sections** — `OptionalSection`/`FeatureRegistry` with dependency-ordered loading, env-var overrides, and graceful degradation
- **Preflight validation** — dry-run every feature section at startup and collect all issues before serving traffic
- **`{{VAR_NAME}}` substitution** — inject values into TOML with pluggable name policies and pluggable value sources

## Basic Usage

```rust,no_run
use configbuilder::{ConfigLoaderFactory, Loader as _};

#[derive(serde::Deserialize, Default)]
struct BrokerConfig { host: String, port: u16 }

let loader = ConfigLoaderFactory::create_loader()?;
let cfg: BrokerConfig = loader.load_section("broker")?;
# Ok::<(), configbuilder::ConfigError>(())
```

`create_loader()` resolves config directories via the XDG chain (`$CONFIGBUILDER_CONFIG_DIR`,
`$XDG_CONFIG_DIRS`, `$XDG_CONFIG_HOME`, falling back to `./config`). Use
`ConfigLoaderFactory::create_loader_for_dir(path)` to read from an explicit directory instead,
or `ConfigLoaderFactory::create_loader_xdg(app_name)` to XDG-resolve under a named app.

## Environment Variable Substitution

Substitute values into TOML using `{{VAR_NAME}}` syntax. Substitution is **opt-in** and
requires an explicit name policy — a placeholder is only substituted if the policy allows
that variable name.

### `AllowAllPolicy` (test-only)

Gated behind the `test-utils` feature; never use in production.

```rust,no_run
use configbuilder::{AllowAllPolicy, ConfigLoaderFactory};

let loader = ConfigLoaderFactory::create_loader_for_dir_with_substitution(
    "config/",
    Box::new(AllowAllPolicy),
);
```

### `PrefixWhitelistPolicy` (recommended default)

Restrict substitution to variable names with an allowed prefix:

```rust,no_run
use configbuilder::ConfigLoaderFactory;

let policy = ConfigLoaderFactory::create_prefix_whitelist_policy(vec![
    "APP_".to_string(),
    "DB_".to_string(),
]);
let loader = ConfigLoaderFactory::create_loader_for_dir_with_substitution(
    "config/",
    Box::new(policy),
);

// TOML: [db]
//       host = "{{DB_HOST}}"
// OK: DB_HOST matches the DB_ prefix.
// Rejected: {{PRIVATE_KEY}} — no prefix matches.
```

### `PatternWhitelistPolicy` (regex-based)

```rust,no_run
use configbuilder::ConfigLoaderFactory;

let policy = ConfigLoaderFactory::create_pattern_whitelist_policy(
    "^(APP|SERVICE)_[A-Z_]+$".to_string(),
)?;
let loader = ConfigLoaderFactory::create_loader_for_dir_with_substitution(
    "config/",
    Box::new(policy),
);
# Ok::<(), String>(())
```

### `CompositePolicy` (combine policies — any one allowing is enough)

```rust,no_run
use configbuilder::{ConfigLoaderFactory, SubstitutionPolicy};

let policies: Vec<Box<dyn SubstitutionPolicy>> = vec![
    Box::new(ConfigLoaderFactory::create_prefix_whitelist_policy(vec!["APP_".to_string()])),
];
let policy = ConfigLoaderFactory::create_composite_policy(policies);
let loader = ConfigLoaderFactory::create_loader_for_dir_with_substitution(
    "config/",
    Box::new(policy),
);
```

### Pluggable value source: `ValueResolver`

By default, substitution values come from `std::env::var` (`EnvValueResolver`). Supply a
custom `ValueResolver` to source values from a secrets backend or anywhere else instead —
the name policy still gates which variable names are allowed; the resolver only controls
*where the value comes from*:

```rust,no_run
use configbuilder::{ConfigLoaderFactory, SubstitutionError, ValueResolver};

struct StaticResolver;
impl ValueResolver for StaticResolver {
    fn resolve(&self, var_name: &str, location: &str) -> Result<String, SubstitutionError> {
        match var_name {
            "APP_GREETING" => Ok("hello".to_string()),
            _ => Err(SubstitutionError::VariableNotFound {
                var_name: var_name.to_string(),
                location: location.to_string(),
            }),
        }
    }
}

let policy = ConfigLoaderFactory::create_prefix_whitelist_policy(vec!["APP_".to_string()]);
let loader = ConfigLoaderFactory::create_loader_for_dir_with_resolver(
    "config/",
    Box::new(policy),
    Box::new(StaticResolver),
);
```

### Escaping Literal Braces

To use literal `{{`/`}}` without substitution, escape them: `\{\{VAR_NAME\}\}` renders as
`{{VAR_NAME}}`.

## Preflight Validation

Dry-run every feature section, collecting **all** issues instead of stopping at the first:

```rust,no_run
use configbuilder::{preflight, ConfigLoaderFactory, OptionalSection, PreflightReportOps as _};

# #[derive(serde::Deserialize)] struct CacheConfig;
# impl OptionalSection for CacheConfig { fn section_name() -> &'static str { "cache" } }
let loader = ConfigLoaderFactory::create_loader_for_dir("config/");
let report = preflight!(&loader, CacheConfig);

if !report.is_ok() {
    eprintln!("{report}");
    std::process::exit(1);
}
```

## Optional Feature Sections

Load a set of `OptionalSection` types in dependency order via `load_in_order!`, or manage a
`FeatureRegistry` directly for observer hooks and startup summaries — see the
[`ConfigLoaderFactory`] and [`OptionalSection`] rustdoc for the full API.

## Builder Pattern

For more control over directories and substitution together:

```rust,no_run
use configbuilder::{BuilderFinalizer as _, ConfigBuilder as _, ConfigLoaderFactory};

let policy = ConfigLoaderFactory::create_prefix_whitelist_policy(vec!["APP_".to_string()]);
let loader = ConfigLoaderFactory::create_config_builder_with_substitution(Box::new(policy))
    .with_config_dir("/etc/myapp")
    .build_loader()?;
# Ok::<(), configbuilder::ConfigError>(())
```

Every directory is searched for a file named `application.toml` by default. Override the
filename with `with_config_filename`:

```rust,no_run
use configbuilder::{BuilderFinalizer as _, ConfigBuilder as _, ConfigLoaderFactory};

let loader = ConfigLoaderFactory::create_config_builder()
    .with_config_dir("/etc/myapp")
    .with_config_filename("settings.toml")
    .build_loader()?;
# Ok::<(), configbuilder::ConfigError>(())
```

## Documentation

| Document | Description |
|----------|-------------|
| [Overview](docs/README.md) | WHAT + WHY — capabilities and design rationale |
| [Architecture](docs/architecture.md) | SEA module layout, data flow, key contracts |
| [Rustdoc](https://docs.rs/configbuilder) | Full API reference |

[`ConfigLoaderFactory`]: https://docs.rs/configbuilder/latest/configbuilder/struct.ConfigLoaderFactory.html
[`OptionalSection`]: https://docs.rs/configbuilder/latest/configbuilder/trait.OptionalSection.html
