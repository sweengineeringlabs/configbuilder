//! `DefaultSectionLoader` — layered TOML section extractor with optional substitution.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

const MAX_CONFIG_FILE_BYTES: u64 = 1_048_576;
const NOT_A_DIR_MSG: &str = "config path exists but is not a directory";

/// Default wall-clock deadline for a single `application.toml` read.
///
/// 30 seconds is generous enough for a slow spinning disk while still bounding
/// the worst-case startup hang from a stalled NFS/FUSE mount.
pub(crate) const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Default config filename searched for in each configured directory.
pub(crate) const DEFAULT_CONFIG_FILENAME: &str = "application.toml";
use crate::api::{
    ConfigError, FeatureLoader, FeatureMetadata, FeatureRecord, FeatureState, LoadedFeature,
    Loader, LoaderOps, OnError, OptionalSection, OverrideSource, Preflight, RawFeature,
    SectionLoaderBound, SubstitutionPolicy, ValueResolver,
};
use crate::core::Substituter;
use crate::EnvValueResolver;

static DEFAULT_VALUE_RESOLVER: EnvValueResolver = EnvValueResolver;

/// Loads an arbitrary TOML section from a layered chain of config directories.
///
/// Each directory's `application.toml` is merged in order; later entries win.
/// Merging is **recursive**: when both the base and overlay contain a TOML table at
/// the same key path, their sub-keys are merged rather than the overlay replacing
/// the entire table. Arrays and scalars are always replaced by the overlay value.
///
/// Optionally applies environment variable substitution (`{{VAR_NAME}}` syntax)
/// if created with a substitution policy.
pub(crate) struct DefaultSectionLoader {
    pub(crate) config_dirs: Vec<PathBuf>,
    pub(crate) substitution_policy: Option<Box<dyn SubstitutionPolicy>>,
    /// Value source for `{{VAR_NAME}}` substitution. Defaults to
    /// [`EnvValueResolver`] (`std::env::var`) when `None`.
    pub(crate) value_resolver: Option<Box<dyn ValueResolver>>,
    /// Wall-clock deadline for each config file read.
    pub(crate) read_timeout: Duration,
    /// Filename searched for in each configured directory. Defaults to
    /// [`DEFAULT_CONFIG_FILENAME`] (`application.toml`).
    pub(crate) config_filename: String,
    /// Explicit config files, each of any name, read after every directory in the order added, so they override the directories. A file named here must exist.
    pub(crate) config_files: Vec<PathBuf>,
}

impl DefaultSectionLoader {
    /// The sources of the chain, lowest priority first: `<dir>/<config_filename>` for each directory (skipped when absent), then each explicit file (an error
    /// when absent, because the caller named it).
    fn sources(&self) -> Vec<(PathBuf, bool)> {
        self.config_dirs
            .iter()
            .map(|dir| (dir.join(&self.config_filename), false))
            .chain(self.config_files.iter().map(|file| (file.clone(), true)))
            .collect()
    }

    /// Read a file on a background thread, returning `ConfigError::Io` if the read
    /// does not complete within `timeout`.  The background thread is not cancelled
    /// (there is no portable cancel mechanism for in-flight OS I/O), but it is
    /// detached and will exit naturally once the underlying I/O completes or fails.
    fn read_with_timeout(path: &Path, timeout: Duration) -> Result<String, ConfigError> {
        let path_buf = path.to_path_buf();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(std::fs::read_to_string(&path_buf));
        });
        match rx.recv_timeout(timeout) {
            Ok(Ok(text)) => Ok(text),
            Ok(Err(e)) => Err(ConfigError::Io(format!("{}: {e}", path.display()))),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ConfigError::Io(format!(
                "{}: read timed out after {}s — filesystem may be stalled",
                path.display(),
                timeout.as_secs()
            ))),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ConfigError::Io(format!(
                "{}: reader thread disconnected unexpectedly",
                path.display()
            ))),
        }
    }

    /// Apply `{{VAR_NAME}}` substitution to `text` when a substitution policy is
    /// configured, resolving values via `self.value_resolver` (defaulting to
    /// [`EnvValueResolver`] when unset).
    fn apply_substitution(&self, text: String, location: String) -> Result<String, ConfigError> {
        let Some(ref policy) = self.substitution_policy else {
            return Ok(text);
        };
        let resolver: &dyn ValueResolver = self
            .value_resolver
            .as_deref()
            .unwrap_or(&DEFAULT_VALUE_RESOLVER);
        let substituter = Substituter::new(policy.as_ref(), resolver, location);
        substituter
            .substitute(&text)
            .map_err(|e| ConfigError::Io(e.to_string()))
    }

    fn merge_toml(base: toml::Value, overlay: toml::Value) -> toml::Value {
        match (base, overlay) {
            (toml::Value::Table(mut b), toml::Value::Table(o)) => {
                for (k, v) in o {
                    let merged = match b.remove(&k) {
                        Some(base_v) => Self::merge_toml(base_v, v),
                        None => v,
                    };
                    b.insert(k, merged);
                }
                toml::Value::Table(b)
            }
            (_, o) => o,
        }
    }

    fn extract_dotted(val: &toml::Value, key: &str) -> Option<toml::Value> {
        let mut current = val;
        for part in key.split('.') {
            current = current.get(part)?;
        }
        Some(current.clone())
    }

    /// Convert a TOML section key to its `CONFIGBUILDER_FEATURE_*` env var name.
    fn feature_env_var_name(key: &str) -> String {
        let suffix = key.to_uppercase().replace('.', "_");
        format!("CONFIGBUILDER_FEATURE_{suffix}")
    }

    /// Parse an env var value as a boolean toggle.
    ///
    /// Returns `Some(true/false)` for recognised values, `None` for the empty
    /// string (var not set), and `Err` for unrecognised non-empty values.
    fn parse_feature_env_var(val: &str, var_name: &str) -> Result<Option<bool>, ConfigError> {
        match val.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(Some(true)),
            "false" | "0" | "no" | "off" => Ok(Some(false)),
            _ => Err(ConfigError::Io(format!(
                "invalid value for env var {var_name}: '{val}'; \
                 accepted values: true/false, 1/0, yes/no, on/off"
            ))),
        }
    }

    /// Resolve the `CONFIGBUILDER_FEATURE_*_ON_ERROR` policy for a feature key.
    pub(crate) fn resolve_feature_on_error<T: OptionalSection>(key: &str) -> OnError {
        let var_name = format!(
            "CONFIGBUILDER_FEATURE_{}_ON_ERROR",
            key.to_uppercase().replace('.', "_")
        );
        match std::env::var(&var_name).as_deref() {
            Ok("disable") => OnError::Disable,
            Ok("fail") => OnError::Fail,
            _ => T::on_error(),
        }
    }

    /// Read every `application.toml` across all config dirs, apply substitution,
    /// and merge the section at `key`.  Returns `(section_found, merged_value)`.
    fn scan_and_merge(&self, key: &str) -> Result<(bool, toml::Value), ConfigError> {
        let mut merged = toml::Value::Table(toml::map::Map::new());
        let mut section_found = false;

        for (path, required) in self.sources() {
            if !path.exists() {
                if required {
                    return Err(ConfigError::NotFound(format!(
                        "{}: the config file does not exist",
                        path.display()
                    )));
                }
                continue;
            }
            let meta = std::fs::metadata(&path)
                .map_err(|e| ConfigError::Io(format!("{}: {e}", path.display())))?;
            if meta.len() > MAX_CONFIG_FILE_BYTES {
                return Err(ConfigError::Io(format!(
                    "{}: config file exceeds the 1 MiB limit ({} bytes)",
                    path.display(),
                    meta.len(),
                )));
            }
            let text = Self::read_with_timeout(&path, self.read_timeout)?;

            let text = self.apply_substitution(text, format!("{}:{}", path.display(), key))?;

            let val: toml::Value =
                toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
            if let Some(section) = Self::extract_dotted(&val, key) {
                section_found = true;
                merged = Self::merge_toml(merged, section);
            }
        }

        Ok((section_found, merged))
    }
}

impl Loader for DefaultSectionLoader {
    fn load_section<T>(&self, key: &str) -> Result<T, ConfigError>
    where
        T: serde::de::DeserializeOwned + Default,
    {
        let val = self.load_section_value(key)?;
        // Empty table == section absent but files found — return type default.
        if val.as_table().is_some_and(|t| t.is_empty()) {
            return Ok(T::default());
        }
        val.try_into()
            .map_err(|e: toml::de::Error| ConfigError::Parse(e.to_string()))
    }

    fn validate(&self) -> Result<(), ConfigError> {
        for dir in &self.config_dirs {
            if dir.exists() && !dir.is_dir() {
                return Err(ConfigError::Io(format!(
                    "{}: {NOT_A_DIR_MSG}",
                    dir.display()
                )));
            }
        }
        for file in &self.config_files {
            if file.exists() && !file.is_file() {
                return Err(ConfigError::Io(format!(
                    "{}: the config file is not a file",
                    file.display()
                )));
            }
        }
        Ok(())
    }
}

impl LoaderOps for DefaultSectionLoader {
    /// Load the raw merged TOML value at `key`.
    ///
    /// - Returns `Err(NotFound)` when no `application.toml` exists anywhere.
    /// - Returns `Ok(empty table)` when files exist but the section is absent
    ///   (caller interprets this as "use the type default").
    /// - Returns `Ok(merged value)` when the section is present.
    fn load_section_value(&self, key: &str) -> Result<toml::Value, ConfigError> {
        let mut any_file_found = false;
        let mut merged = toml::Value::Table(toml::map::Map::new());

        for (path, required) in self.sources() {
            if !path.exists() {
                if required {
                    return Err(ConfigError::NotFound(format!(
                        "{}: the config file does not exist",
                        path.display()
                    )));
                }
                continue;
            }
            any_file_found = true;
            let meta = std::fs::metadata(&path)
                .map_err(|e| ConfigError::Io(format!("{}: {e}", path.display())))?;
            if meta.len() > MAX_CONFIG_FILE_BYTES {
                return Err(ConfigError::Io(format!(
                    "{}: config file exceeds the 1 MiB limit ({} bytes)",
                    path.display(),
                    meta.len(),
                )));
            }
            let text = Self::read_with_timeout(&path, self.read_timeout)?;

            let text = self.apply_substitution(text, format!("{}:{}", path.display(), key))?;

            let val: toml::Value =
                toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
            if let Some(section) = Self::extract_dotted(&val, key) {
                merged = Self::merge_toml(merged, section);
            }
        }

        if matches!(merged, toml::Value::Table(ref t) if t.is_empty()) {
            if !any_file_found {
                return Err(ConfigError::NotFound(format!(
                    "no {} found in any configured directory or file for section '{key}'",
                    self.config_filename
                )));
            }
            // Return the empty table as the absent-but-files-found sentinel.
            return Ok(toml::Value::Table(toml::map::Map::new()));
        }

        Ok(merged)
    }

    fn validate_dirs(&self) -> Result<(), ConfigError> {
        <Self as Loader>::validate(self)
    }

    fn load_feature_raw(&self, key: &str) -> Result<RawFeature, ConfigError> {
        let var_name = Self::feature_env_var_name(key);

        // ── 1. Env-var override (highest precedence) ─────────────────────────
        let env_force: Option<bool> = match std::env::var(&var_name) {
            Err(_) => None, // var not set
            Ok(ref val) => Self::parse_feature_env_var(val, &var_name)?,
        };

        if env_force == Some(false) {
            return Ok(RawFeature {
                value: None,
                record: Box::new(FeatureRecord {
                    section_name: key.to_owned(),
                    enabled: false,
                    override_source: Some(OverrideSource::EnvVar {
                        var_name,
                        value: std::env::var(Self::feature_env_var_name(key)).unwrap_or_default(),
                    }),
                    requires: &[],
                    metadata: Box::new(FeatureMetadata::default()),
                }),
            });
        }

        // ── 2. TOML scan ─────────────────────────────────────────────────────
        let (section_found, merged) = self.scan_and_merge(key)?;

        if !section_found {
            if env_force == Some(true) {
                // Operator explicitly requested enable but section is absent.
                return Err(ConfigError::NotFound(format!(
                    "env var {var_name}=true but section '{key}' \
                     is absent from all config files; add [{key}] to application.toml"
                )));
            }
            return Ok(RawFeature {
                value: None,
                record: Box::new(FeatureRecord {
                    section_name: key.to_owned(),
                    enabled: false,
                    override_source: None,
                    requires: &[],
                    metadata: Box::new(FeatureMetadata::default()),
                }),
            });
        }

        // ── 3. Explicit `enabled = false` in TOML (skipped when env forces on) ─
        if env_force != Some(true) {
            if let Some(toml::Value::Boolean(false)) = merged.get("enabled") {
                return Ok(RawFeature {
                    value: None,
                    record: Box::new(FeatureRecord {
                        section_name: key.to_owned(),
                        enabled: false,
                        override_source: Some(OverrideSource::ExplicitTomlFlag),
                        requires: &[],
                        metadata: Box::new(FeatureMetadata::default()),
                    }),
                });
            }
        }

        // ── 4. Feature is enabled — return the raw TOML value ────────────────
        let override_source = env_force.map(|_| OverrideSource::EnvVar {
            var_name,
            value: "true".into(),
        });

        Ok(RawFeature {
            value: Some(merged),
            record: Box::new(FeatureRecord {
                section_name: key.to_owned(),
                enabled: true,
                override_source,
                requires: &[],
                metadata: Box::new(FeatureMetadata::default()),
            }),
        })
    }
}

impl FeatureLoader for DefaultSectionLoader {
    fn load_feature<T>(&self, key: &str) -> Result<LoadedFeature<T>, ConfigError>
    where
        T: serde::de::DeserializeOwned,
    {
        let raw = self.load_feature_raw(key)?;
        let state = match raw.value {
            None => FeatureState::Disabled,
            Some(val) => FeatureState::Enabled(
                val.try_into()
                    .map_err(|e: toml::de::Error| ConfigError::Parse(e.to_string()))?,
            ),
        };
        Ok(LoadedFeature {
            state,
            record: raw.record,
        })
    }
}

impl SectionLoaderBound for DefaultSectionLoader {
    type FeatureRecord = crate::api::FeatureRecord;
    type FeatureRecordBuilder = crate::api::FeatureRecordBuilder;
    type FeatureRegistry = crate::api::FeatureRegistry;
    type FeatureSummary = crate::api::FeatureSummary;
    type OverrideSource = crate::api::OverrideSource;
    type Topology = crate::api::Topology;
}

impl Preflight for DefaultSectionLoader {
    type Issue = crate::api::PreflightIssue;
    type IssueKind = crate::api::PreflightIssueKind;
    type Report = crate::api::PreflightReport;
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use crate::api::{ConfigError, FeatureState};
    use std::io::Write as _;
    use std::path::Path;
    use tempfile::TempDir;

    #[derive(Debug, Default, serde::Deserialize, PartialEq)]
    #[serde(default)]
    struct DefaultSectionLoaderSection {
        value: String,
        count: u32,
    }

    impl OptionalSection for DefaultSectionLoaderSection {
        fn section_name() -> &'static str {
            Box::leak(String::from("env_fallback").into_boxed_str())
        }

        fn on_error() -> OnError {
            OnError::Fail
        }
    }

    fn must<T, E>(result: Result<T, E>) -> T {
        result.unwrap_or_else(|_| std::process::abort())
    }

    fn must_err<T, E>(result: Result<T, E>) -> E {
        match result {
            Ok(_) => std::process::abort(),
            Err(err) => err,
        }
    }

    fn some<T>(option: Option<T>) -> T {
        match option {
            Some(value) => value,
            None => std::process::abort(),
        }
    }

    fn loader_in(dir: &Path) -> DefaultSectionLoader {
        DefaultSectionLoader {
            config_dirs: vec![dir.to_path_buf()],
            substitution_policy: None,
            value_resolver: None,
            read_timeout: DEFAULT_READ_TIMEOUT,
            config_filename: DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        }
    }

    fn write_toml(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            must(std::fs::create_dir_all(parent));
        }
        let mut file = must(std::fs::File::create(&path));
        must(file.write_all(content.as_bytes()));
    }

    #[test]
    fn test_load_section_reads_top_level_key() {
        let dir = must(TempDir::new());
        write_toml(
            dir.path(),
            "application.toml",
            "[my_section]\nvalue = \"hello\"\ncount = 7",
        );
        let sec: DefaultSectionLoaderSection =
            must(loader_in(dir.path()).load_section("my_section"));
        assert_eq!(sec.value, "hello");
        assert_eq!(sec.count, 7);
    }

    #[test]
    fn test_load_section_returns_not_found_when_no_application_toml() {
        let dir = must(TempDir::new());
        let result: Result<DefaultSectionLoaderSection, _> =
            loader_in(dir.path()).load_section("nonexistent");
        assert!(
            matches!(result, Err(ConfigError::NotFound(_))),
            "expected NotFound when no application.toml exists, got {result:?}"
        );
    }

    #[test]
    fn test_load_section_returns_default_when_section_absent_from_existing_toml() {
        let dir = must(TempDir::new());
        write_toml(
            dir.path(),
            "application.toml",
            "[other_section]\nvalue = \"x\"",
        );
        let sec: DefaultSectionLoaderSection =
            must(loader_in(dir.path()).load_section("nonexistent"));
        assert_eq!(sec, DefaultSectionLoaderSection::default());
    }

    #[test]
    fn test_load_section_supports_dotted_key_path() {
        let dir = must(TempDir::new());
        write_toml(
            dir.path(),
            "application.toml",
            "[outer.inner]\nvalue = \"deep\"\ncount = 3",
        );
        let sec: DefaultSectionLoaderSection =
            must(loader_in(dir.path()).load_section("outer.inner"));
        assert_eq!(sec.value, "deep");
        assert_eq!(sec.count, 3);
    }

    #[test]
    fn test_load_section_later_dir_wins_over_earlier() {
        let low = must(TempDir::new());
        let high = must(TempDir::new());
        write_toml(low.path(), "application.toml", "[s]\nvalue = \"low\"");
        write_toml(high.path(), "application.toml", "[s]\nvalue = \"high\"");
        let loader = DefaultSectionLoader {
            config_dirs: vec![low.path().to_path_buf(), high.path().to_path_buf()],
            substitution_policy: None,
            value_resolver: None,
            read_timeout: DEFAULT_READ_TIMEOUT,
            config_filename: DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        };
        let sec: DefaultSectionLoaderSection = must(loader.load_section("s"));
        assert_eq!(sec.value, "high");
    }

    #[test]
    fn test_load_section_earlier_dir_fills_unset_fields() {
        let low = must(TempDir::new());
        let high = must(TempDir::new());
        write_toml(low.path(), "application.toml", "[s]\ncount = 9");
        write_toml(high.path(), "application.toml", "[s]\nvalue = \"hi\"");
        let loader = DefaultSectionLoader {
            config_dirs: vec![low.path().to_path_buf(), high.path().to_path_buf()],
            substitution_policy: None,
            value_resolver: None,
            read_timeout: DEFAULT_READ_TIMEOUT,
            config_filename: DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        };
        let sec: DefaultSectionLoaderSection = must(loader.load_section("s"));
        assert_eq!(sec.value, "hi");
        assert_eq!(sec.count, 9);
    }

    #[derive(Debug, Default, serde::Deserialize, PartialEq)]
    #[serde(default)]
    struct DefaultSectionLoaderServer {
        host: String,
        tls: DefaultSectionLoaderTls,
    }

    #[derive(Debug, Default, serde::Deserialize, PartialEq)]
    #[serde(default)]
    struct DefaultSectionLoaderTls {
        cert: String,
        key: String,
    }

    #[test]
    fn test_load_section_deep_merges_nested_tables_across_dirs() {
        // Regression: shallow merge replaced the entire `tls` subtable, dropping
        // any key in the base that was absent from the overlay.
        let low = must(TempDir::new());
        let high = must(TempDir::new());
        write_toml(
            low.path(),
            "application.toml",
            "[s]\nhost = \"localhost\"\n\n[s.tls]\ncert = \"old.pem\"\nkey = \"key.pem\"",
        );
        write_toml(
            high.path(),
            "application.toml",
            "[s.tls]\ncert = \"new.pem\"",
        );
        let loader = DefaultSectionLoader {
            config_dirs: vec![low.path().to_path_buf(), high.path().to_path_buf()],
            substitution_policy: None,
            value_resolver: None,
            read_timeout: DEFAULT_READ_TIMEOUT,
            config_filename: DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        };
        let srv: DefaultSectionLoaderServer = must(loader.load_section("s"));
        assert_eq!(
            srv.host, "localhost",
            "host must survive overlay of sibling subtable"
        );
        assert_eq!(
            srv.tls.cert, "new.pem",
            "cert must be overridden by high-priority dir"
        );
        assert_eq!(
            srv.tls.key, "key.pem",
            "key must not be lost when only cert is overridden"
        );
    }

    #[test]
    fn test_load_section_rejects_oversized_application_toml() {
        let dir = must(TempDir::new());
        let oversized = vec![b'#'; (MAX_CONFIG_FILE_BYTES + 1) as usize];
        must(std::fs::write(
            dir.path().join("application.toml"),
            &oversized,
        ));
        let err = must_err(loader_in(dir.path()).load_section::<DefaultSectionLoaderSection>("s"));
        assert!(matches!(err, ConfigError::Io(_)));
        assert!(err.to_string().contains("1 MiB"));
    }

    #[test]
    fn test_load_section_rejects_invalid_toml() {
        let dir = must(TempDir::new());
        write_toml(dir.path(), "application.toml", "not = [broken toml");
        let err = must_err(loader_in(dir.path()).load_section::<DefaultSectionLoaderSection>("s"));
        assert!(matches!(err, ConfigError::Parse(_)));
    }

    // ── load_optional_section ────────────────────────────────────────────────

    #[test]
    fn test_load_optional_section_present_key_returns_enabled() {
        let dir = must(TempDir::new());
        write_toml(
            dir.path(),
            "application.toml",
            "[my_feature]\nvalue = \"on\"\ncount = 3",
        );
        let state: FeatureState<DefaultSectionLoaderSection> =
            must(loader_in(dir.path()).load_optional_section("my_feature"));
        assert!(state.is_enabled(), "expected Enabled when key is present");
        let sec = some(state.into_option());
        assert_eq!(sec.value, "on");
        assert_eq!(sec.count, 3);
    }

    #[test]
    fn test_load_optional_section_absent_key_in_existing_file_returns_disabled() {
        let dir = must(TempDir::new());
        write_toml(
            dir.path(),
            "application.toml",
            "[other_section]\nvalue = \"x\"",
        );
        let state: FeatureState<DefaultSectionLoaderSection> =
            must(loader_in(dir.path()).load_optional_section("my_feature"));
        assert!(
            state.is_disabled(),
            "expected Disabled when key absent from existing file"
        );
    }

    #[test]
    fn test_load_optional_section_no_files_returns_disabled() {
        let dir = must(TempDir::new());
        let state: FeatureState<DefaultSectionLoaderSection> =
            must(loader_in(dir.path()).load_optional_section("my_feature"));
        assert!(
            state.is_disabled(),
            "expected Disabled when no config files exist"
        );
    }

    #[test]
    fn test_load_optional_section_malformed_toml_returns_parse_error() {
        let dir = must(TempDir::new());
        write_toml(dir.path(), "application.toml", "not = [broken toml");
        let err = must_err(
            loader_in(dir.path())
                .load_optional_section::<DefaultSectionLoaderSection>("my_feature"),
        );
        assert!(
            matches!(err, ConfigError::Parse(_)),
            "expected Parse error for malformed TOML, got {err:?}"
        );
    }

    #[test]
    fn test_load_optional_section_missing_required_field_returns_parse_error() {
        #[derive(Debug, serde::Deserialize)]
        struct DefaultSectionLoaderStrict {
            #[expect(
                dead_code,
                reason = "deserialization target — its absence is the test subject"
            )]
            required: String, // no Default, no Option — must be present
        }
        let dir = must(TempDir::new());
        // Section present but `required` field is absent
        write_toml(dir.path(), "application.toml", "[feat]\nother = \"x\"");
        let err = must_err(
            loader_in(dir.path()).load_optional_section::<DefaultSectionLoaderStrict>("feat"),
        );
        assert!(
            matches!(err, ConfigError::Parse(_)),
            "expected Parse for missing required field, got {err:?}"
        );
    }

    #[test]
    fn test_load_optional_section_multi_dir_merge_returns_enabled() {
        let low = must(TempDir::new());
        let high = must(TempDir::new());
        write_toml(low.path(), "application.toml", "[feat]\ncount = 1");
        write_toml(high.path(), "application.toml", "[feat]\nvalue = \"hi\"");
        let loader = DefaultSectionLoader {
            config_dirs: vec![low.path().to_path_buf(), high.path().to_path_buf()],
            substitution_policy: None,
            value_resolver: None,
            read_timeout: DEFAULT_READ_TIMEOUT,
            config_filename: DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        };
        let state: FeatureState<DefaultSectionLoaderSection> =
            must(loader.load_optional_section("feat"));
        assert!(state.is_enabled());
        let sec = some(state.into_option());
        assert_eq!(sec.value, "hi");
        assert_eq!(sec.count, 1);
    }

    #[test]
    fn test_load_optional_section_multi_dir_both_absent_returns_disabled() {
        let low = must(TempDir::new());
        let high = must(TempDir::new());
        write_toml(low.path(), "application.toml", "[other]\nvalue = \"x\"");
        write_toml(
            high.path(),
            "application.toml",
            "[also_other]\nvalue = \"y\"",
        );
        let loader = DefaultSectionLoader {
            config_dirs: vec![low.path().to_path_buf(), high.path().to_path_buf()],
            substitution_policy: None,
            value_resolver: None,
            read_timeout: DEFAULT_READ_TIMEOUT,
            config_filename: DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        };
        let state: FeatureState<DefaultSectionLoaderSection> =
            must(loader.load_optional_section("feat"));
        assert!(state.is_disabled());
    }

    // ── load_feature ─────────────────────────────────────────────────────────

    // Env-var tests mutate process state — serialize them.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_load_feature_section_present_returns_enabled_record() {
        let dir = must(TempDir::new());
        write_toml(
            dir.path(),
            "application.toml",
            "[feat]\nvalue = \"on\"\ncount = 5",
        );
        let loaded =
            must(loader_in(dir.path()).load_feature::<DefaultSectionLoaderSection>("feat"));
        assert!(loaded.state.is_enabled());
        assert!(loaded.record.enabled);
        assert!(loaded.record.override_source.is_none());
        assert_eq!(loaded.record.section_name, "feat");
    }

    #[test]
    fn test_load_feature_section_absent_returns_disabled_record() {
        let dir = must(TempDir::new());
        write_toml(dir.path(), "application.toml", "[other]\nvalue = \"x\"");
        let loaded =
            must(loader_in(dir.path()).load_feature::<DefaultSectionLoaderSection>("feat"));
        assert!(loaded.state.is_disabled());
        assert!(!loaded.record.enabled);
        assert!(loaded.record.override_source.is_none());
    }

    #[test]
    fn test_load_feature_enabled_false_in_toml_returns_disabled_with_explicit_flag() {
        let dir = must(TempDir::new());
        write_toml(
            dir.path(),
            "application.toml",
            "[feat]\nenabled = false\nvalue = \"x\"",
        );
        let loaded =
            must(loader_in(dir.path()).load_feature::<DefaultSectionLoaderSection>("feat"));
        assert!(loaded.state.is_disabled());
        assert!(!loaded.record.enabled);
        assert!(
            matches!(
                loaded.record.override_source,
                Some(crate::api::OverrideSource::ExplicitTomlFlag)
            ),
            "expected ExplicitTomlFlag override source"
        );
    }

    #[test]
    fn test_load_feature_env_var_false_disables_present_section() {
        let _g = must(ENV_LOCK.lock());
        let var = "CONFIGBUILDER_FEATURE_FEAT_LF_ENV_OFF";
        // SAFETY: test-only, serialized by ENV_LOCK
        unsafe { std::env::set_var(var, "false") };
        let dir = must(TempDir::new());
        write_toml(
            dir.path(),
            "application.toml",
            "[feat_lf_env_off]\nvalue = \"on\"",
        );
        let result =
            loader_in(dir.path()).load_feature::<DefaultSectionLoaderSection>("feat_lf_env_off");
        // SAFETY: cleanup
        unsafe { std::env::remove_var(var) };
        let loaded = must(result);
        assert!(loaded.state.is_disabled());
        assert!(!loaded.record.enabled);
        assert!(
            matches!(
                loaded.record.override_source,
                Some(crate::api::OverrideSource::EnvVar { .. })
            ),
            "expected EnvVar override source"
        );
    }

    #[test]
    fn test_load_feature_env_var_true_enables_present_section() {
        let _g = must(ENV_LOCK.lock());
        let var = "CONFIGBUILDER_FEATURE_FEAT_LF_ENV_ON";
        // SAFETY: test-only, serialized by ENV_LOCK
        unsafe { std::env::set_var(var, "true") };
        let dir = must(TempDir::new());
        write_toml(
            dir.path(),
            "application.toml",
            "[feat_lf_env_on]\nvalue = \"on\"",
        );
        let result =
            loader_in(dir.path()).load_feature::<DefaultSectionLoaderSection>("feat_lf_env_on");
        // SAFETY: cleanup
        unsafe { std::env::remove_var(var) };
        let loaded = must(result);
        assert!(loaded.state.is_enabled());
        assert!(loaded.record.enabled);
        assert!(
            matches!(
                loaded.record.override_source,
                Some(crate::api::OverrideSource::EnvVar { .. })
            ),
            "expected EnvVar override source"
        );
    }

    #[test]
    fn test_load_feature_env_var_true_overrides_enabled_false_in_toml() {
        let _g = must(ENV_LOCK.lock());
        let var = "CONFIGBUILDER_FEATURE_FEAT_LF_FORCE_ON";
        // SAFETY: test-only, serialized by ENV_LOCK
        unsafe { std::env::set_var(var, "1") };
        let dir = must(TempDir::new());
        write_toml(
            dir.path(),
            "application.toml",
            "[feat_lf_force_on]\nenabled = false\nvalue = \"x\"",
        );
        let result =
            loader_in(dir.path()).load_feature::<DefaultSectionLoaderSection>("feat_lf_force_on");
        // SAFETY: cleanup
        unsafe { std::env::remove_var(var) };
        let loaded = must(result);
        assert!(
            loaded.state.is_enabled(),
            "env var=true must override enabled=false in TOML"
        );
    }

    #[test]
    fn test_load_feature_env_var_true_section_absent_returns_not_found() {
        let _g = must(ENV_LOCK.lock());
        let var = "CONFIGBUILDER_FEATURE_FEAT_LF_ABSENT";
        // SAFETY: test-only, serialized by ENV_LOCK
        unsafe { std::env::set_var(var, "true") };
        let dir = must(TempDir::new());
        write_toml(dir.path(), "application.toml", "[other]\nvalue = \"x\"");
        let result =
            loader_in(dir.path()).load_feature::<DefaultSectionLoaderSection>("feat_lf_absent");
        // SAFETY: cleanup
        unsafe { std::env::remove_var(var) };
        assert!(
            matches!(result, Err(ConfigError::NotFound(_))),
            "env var=true + absent section must be NotFound, got {result:?}"
        );
    }

    #[test]
    fn test_load_feature_invalid_env_var_value_returns_io_error() {
        let _g = must(ENV_LOCK.lock());
        let var = "CONFIGBUILDER_FEATURE_FEAT_LF_INVALID";
        // SAFETY: test-only, serialized by ENV_LOCK
        unsafe { std::env::set_var(var, "maybe") };
        let dir = must(TempDir::new());
        write_toml(
            dir.path(),
            "application.toml",
            "[feat_lf_invalid]\nvalue = \"x\"",
        );
        let result =
            loader_in(dir.path()).load_feature::<DefaultSectionLoaderSection>("feat_lf_invalid");
        // SAFETY: cleanup
        unsafe { std::env::remove_var(var) };
        assert!(
            matches!(result, Err(ConfigError::Io(_))),
            "invalid env var value must return Io error, got {result:?}"
        );
    }

    #[test]
    fn test_resolve_feature_on_error_prefers_env_override_over_trait_default() {
        let _g = must(ENV_LOCK.lock());
        let var = "CONFIGBUILDER_FEATURE_ENV_FALLBACK_ON_ERROR";
        // SAFETY: test-only, serialized by ENV_LOCK
        unsafe { std::env::set_var(var, "disable") };
        let resolved = DefaultSectionLoader::resolve_feature_on_error::<DefaultSectionLoaderSection>(
            "env_fallback",
        );
        // SAFETY: cleanup
        unsafe { std::env::remove_var(var) };
        assert!(matches!(resolved, OnError::Disable));
    }

    #[test]
    fn test_validate_accepts_nonexistent_dir() {
        let path = PathBuf::from("/nonexistent/example-app-test-xyz");
        assert!(!path.exists(), "test path must remain absent");
        let loader = DefaultSectionLoader {
            config_dirs: vec![path],
            substitution_policy: None,
            value_resolver: None,
            read_timeout: DEFAULT_READ_TIMEOUT,
            config_filename: DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        };
        assert!(matches!(loader.validate(), Ok(())));
    }

    #[test]
    fn test_validate_accepts_existing_dir() {
        let dir = must(TempDir::new());
        let loader = DefaultSectionLoader {
            config_dirs: vec![dir.path().to_path_buf()],
            substitution_policy: None,
            value_resolver: None,
            read_timeout: DEFAULT_READ_TIMEOUT,
            config_filename: DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        };
        assert_eq!(loader.config_dirs, vec![dir.path().to_path_buf()]);
        assert!(loader.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_file_path() {
        let dir = must(TempDir::new());
        let file = dir.path().join("not_a_dir.toml");
        must(std::fs::write(&file, b""));
        let loader = DefaultSectionLoader {
            config_dirs: vec![file.clone()],
            substitution_policy: None,
            value_resolver: None,
            read_timeout: DEFAULT_READ_TIMEOUT,
            config_filename: DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        };
        let err = must_err(loader.validate());
        assert!(matches!(err, ConfigError::Io(_)));
        assert!(err.to_string().contains("not a directory"));
    }
}
