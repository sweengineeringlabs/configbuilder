# Configuration method examples

One runnable, self-verifying example per way to construct and configure a
loader. Each is a real `cargo run --example` target — not a doc fragment —
that builds its own temp config, exercises exactly one method, and asserts
the result. Run any of them from `scm/`:

```
cargo run --example docs_with_config_filename
```

## `ConfigLoaderFactory` constructors

| Example | Method | Demonstrates |
|---------|--------|--------------|
| `docs_create_loader` | `create_loader()` | XDG-resolved loader, no app name |
| `docs_create_loader_for_dir` | `create_loader_for_dir(dir)` | Loader scoped to one explicit directory |
| `docs_create_loader_xdg` | `create_loader_xdg(app_name)` | XDG-resolved loader for a named app |
| `docs_create_loader_with_substitution` | `create_loader_with_substitution(policy)` | XDG-resolved loader + `{{VAR}}` substitution |
| `docs_create_loader_for_dir_with_substitution` | `create_loader_for_dir_with_substitution(dir, policy)` | Explicit dir + `{{VAR}}` substitution |
| `docs_create_loader_xdg_with_substitution` | `create_loader_xdg_with_substitution(app_name, policy)` | Named app + `{{VAR}}` substitution |
| `docs_create_loader_with_resolver` | `create_loader_with_resolver(policy, resolver)` | XDG-resolved loader + custom `ValueResolver` |
| `docs_create_loader_for_dir_with_resolver` | `create_loader_for_dir_with_resolver(dir, policy, resolver)` | Explicit dir + custom `ValueResolver` |
| `docs_create_loader_xdg_with_resolver` | `create_loader_xdg_with_resolver(app_name, policy, resolver)` | Named app + custom `ValueResolver` |
| `docs_create_config_builder` | `create_config_builder()` | Fluent builder-chain entry point |
| `docs_create_config_builder_with_substitution` | `create_config_builder_with_substitution(policy)` | Builder-chain entry point + `{{VAR}}` substitution |

## `ConfigBuilder` / `ConfigBuilderInit` fluent setters

All chained from `ConfigLoaderFactory::create_config_builder()`.

| Example | Method | Demonstrates |
|---------|--------|--------------|
| `docs_with_name` | `with_name(name)` | Drives XDG-resolved directory derivation |
| `docs_with_version` | `with_version(version)` | Metadata only — no effect on resolution |
| `docs_with_config_dir` | `with_config_dir(dir)` | Accumulates directories; later calls win on key conflicts |
| `docs_with_config_filename` | `with_config_filename(name)` | Overrides the filename searched for (default `application.toml`) |
| `docs_with_config_file` | `with_config_file(file)` | Adds one file of any name after the directories; it overrides them and must exist |
| `docs_with_read_timeout` | `with_read_timeout(duration)` | Overrides the 30s default read deadline |

Every example that depends on XDG/env-based resolution sets
`$CONFIGBUILDER_CONFIG_DIR` to a temp directory first, so they're deterministic
regardless of the machine they run on, and clears it afterward.
