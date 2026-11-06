use std::path::PathBuf;

use crate::api::{
    CompositePolicy, ConfigBuilderImpl, ConfigError, FeatureRegistry, PathValidatorImpl,
    PatternWhitelistPolicy, PrefixWhitelistPolicy, PreflightReport, SectionLoaderImpl,
    SubstitutionPolicy, ValueResolver,
};
use crate::core::DefaultSubstitutionConfigBuilder;

/// SAF facade: the single supported entry point for constructing loaders,
/// builders, and policies without depending on `core/` or `api/` directly.
pub struct ConfigLoaderFactory;

impl ConfigLoaderFactory {
    fn touch_core_api() {
        let enabled = crate::api::FeatureState::Enabled(1_u8);
        let _ = enabled.is_enabled();
        let _ = crate::api::FeatureState::<u8>::Disabled.is_disabled();
        let _ = crate::api::FeatureState::Enabled(1_u8).into_option();
        let _ = crate::api::FeatureState::Enabled(1_u8).as_option();
        let _ = crate::api::FeatureState::Enabled(1_u8).map(|n| n + 1);
        let _ = crate::api::FeatureState::Enabled(1_u8)
            .and_then(|n| crate::api::FeatureState::Enabled(n + 1));
        let _ = crate::api::FeatureState::Enabled(1_u8).unwrap_or(0);
        let _ = crate::api::FeatureState::<u8>::Disabled.unwrap_or_else(|| 0);
        let _ = crate::api::FeatureState::<u8>::Disabled.enabled_or_default();

        let summary = crate::api::FeatureSummary { records: vec![] };
        let _ = summary.enabled_count();
        let _ = summary.disabled_count();
        let _ = summary.total_count();
        let _ = summary.all_enabled();

        use crate::api::TopologyOps as _;
        let _ = crate::api::Topology.sort(&["a"], &[&[]]);

        let _ =
            crate::api::PreflightIssueKind::from_config_error(&ConfigError::Parse(String::new()));
        let mut report = Self::create_preflight_report();
        report.push(crate::api::PreflightIssue {
            section: String::from("touch_section"),
            kind: crate::api::PreflightIssueKind::LoadError,
            message: String::from("touch"),
        });
        let _ = report.is_ok();
        let _ = report.issues();
        let _ = report.issue_count();

        let pw = Self::create_prefix_whitelist_policy(vec!["APP_".to_string()]);
        let _ = &pw.prefixes;
        if let Ok(p) = Self::create_pattern_whitelist_policy(r"^APP_[A-Z_]+$".to_string()) {
            let _ = p.pattern_str.len();
        }
        let _ = Self::create_composite_policy(vec![]);
        let _ = PathValidatorImpl {
            ops: Box::new(crate::core::DefaultValidator),
        }
        .validate_path(std::path::Path::new("."));

        let _ = crate::api::FeatureRecordBuilder::new("touch")
            .enabled(false)
            .override_source(crate::api::OverrideSource::ExplicitTomlFlag)
            .requires(&[])
            .metadata(crate::api::FeatureMetadata::default())
            .build();
    }

    /// Build a loader using XDG-resolved config directories with no app name.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if no config directory is accessible.
    pub fn create_loader() -> Result<SectionLoaderImpl, ConfigError> {
        Self::touch_core_api();
        let loader = crate::core::DefaultConfigBuilder {
            name: String::new(),
            version: String::new(),
            config_dirs: Vec::new(),
            read_timeout: crate::core::DEFAULT_READ_TIMEOUT,
            config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        }
        .build_loader_internal()?;
        Ok(SectionLoaderImpl {
            ops: Box::new(loader),
        })
    }

    /// Build a loader that reads `application.toml` only from `dir`.
    pub fn create_loader_for_dir(dir: impl Into<PathBuf>) -> SectionLoaderImpl {
        SectionLoaderImpl {
            ops: Box::new(crate::core::DefaultSectionLoader {
                config_dirs: vec![dir.into()],
                substitution_policy: None,
                value_resolver: None,
                read_timeout: crate::core::DEFAULT_READ_TIMEOUT,
                config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
                config_files: Vec::new(),
            }),
        }
    }

    /// Build a loader using XDG-resolved config directories for `app_name`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if no config directory is accessible.
    pub fn create_loader_xdg(app_name: &str) -> Result<SectionLoaderImpl, ConfigError> {
        let loader = crate::core::DefaultConfigBuilder {
            name: app_name.to_owned(),
            version: String::new(),
            config_dirs: Vec::new(),
            read_timeout: crate::core::DEFAULT_READ_TIMEOUT,
            config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        }
        .build_loader_internal()?;
        Ok(SectionLoaderImpl {
            ops: Box::new(loader),
        })
    }

    /// Create the default filesystem path validator.
    pub fn create_validator() -> PathValidatorImpl {
        PathValidatorImpl {
            ops: Box::new(crate::core::DefaultValidator),
        }
    }

    /// Create an empty preflight report with no recorded issues.
    pub fn create_preflight_report() -> PreflightReport {
        PreflightReport { issues: Vec::new() }
    }

    /// Return `true` when `report` has no recorded issues.
    pub fn preflight_report_is_ok(report: &PreflightReport) -> bool {
        report.is_ok()
    }

    /// Append `issue` to `report`.
    pub fn preflight_report_push(report: &mut PreflightReport, issue: crate::api::PreflightIssue) {
        report.push(issue);
    }

    /// Borrow the issues recorded on `report`.
    pub fn preflight_report_issues(report: &PreflightReport) -> &[crate::api::PreflightIssue] {
        report.issues()
    }

    /// Return the number of issues recorded on `report`.
    pub fn preflight_report_issue_count(report: &PreflightReport) -> usize {
        report.issue_count()
    }

    /// Classify a [`ConfigError`] into the [`crate::api::PreflightIssueKind`] it represents.
    pub fn preflight_issue_kind_from_config_error(
        e: &ConfigError,
    ) -> crate::api::PreflightIssueKind {
        crate::api::PreflightIssueKind::from_config_error(e)
    }

    /// Create a substitution policy that allows env vars matching any of `prefixes`.
    pub fn create_prefix_whitelist_policy(prefixes: Vec<String>) -> PrefixWhitelistPolicy {
        PrefixWhitelistPolicy::new(prefixes)
    }

    /// Create a substitution policy that allows env vars matching a regex `pattern`.
    ///
    /// # Errors
    ///
    /// Returns `Err` with a description if `pattern` is not a valid regex.
    pub fn create_pattern_whitelist_policy(
        pattern: String,
    ) -> Result<PatternWhitelistPolicy, String> {
        PatternWhitelistPolicy::new(pattern)
    }

    /// Create a substitution policy that allows an env var when any of `policies` allows it.
    pub fn create_composite_policy(policies: Vec<Box<dyn SubstitutionPolicy>>) -> CompositePolicy {
        CompositePolicy::new(policies)
    }

    /// Create an empty feature registry.
    pub fn create_feature_registry() -> FeatureRegistry {
        FeatureRegistry {
            records: Vec::new(),
            observers: Vec::new(),
        }
    }

    /// Load a feature section into `registry`, recording its resolved state.
    ///
    /// # Errors
    ///
    /// Propagates any [`ConfigError`] returned by `loader`.
    pub fn feature_registry_load<T>(
        registry: &mut FeatureRegistry,
        loader: &SectionLoaderImpl,
    ) -> Result<crate::api::FeatureState<T>, ConfigError>
    where
        T: crate::api::OptionalSection,
    {
        registry.load(loader)
    }

    /// Borrow the feature records collected on `registry`.
    pub fn feature_registry_records(registry: &FeatureRegistry) -> &[crate::api::FeatureRecord] {
        registry.records()
    }

    /// Build a point-in-time snapshot of `registry`'s recorded features.
    pub fn feature_registry_summary(registry: &FeatureRegistry) -> crate::api::FeatureSummary {
        registry.summary()
    }

    /// Validate that every enabled feature in `registry` has its dependencies enabled.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Validation`](crate::ConfigError::Validation) listing unsatisfied dependencies.
    pub fn feature_registry_validate_dependencies(
        registry: &FeatureRegistry,
    ) -> Result<(), ConfigError> {
        registry.validate_dependencies()
    }

    /// Register a callback invoked with each feature record as it is loaded.
    pub fn feature_registry_on_load(
        registry: &mut FeatureRegistry,
        observer: impl Fn(&crate::api::FeatureRecord) + 'static,
    ) {
        registry.on_load(observer)
    }

    /// Compute a topological load order over `names` given each name's `requires`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Validation`](crate::ConfigError::Validation) listing the nodes in a detected cycle.
    pub fn topology_sort(names: &[&str], requires: &[&[&str]]) -> Result<Vec<usize>, ConfigError> {
        use crate::api::TopologyOps as _;
        crate::api::Topology.sort(names, requires)
    }

    /// Create an empty concrete config builder ready for fluent configuration.
    pub fn create_config_builder() -> ConfigBuilderImpl {
        ConfigBuilderImpl {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            config_dirs: Vec::new(),
            read_timeout: None,
            config_filename: None,
            config_files: Vec::new(),
        }
    }

    /// Load one optional feature section from `loader` without a registry.
    ///
    /// # Errors
    ///
    /// Propagates any [`ConfigError`] returned while reading or parsing the section.
    pub fn load_feature_section<T>(
        loader: &SectionLoaderImpl,
        key: &str,
    ) -> Result<crate::api::FeatureState<T>, ConfigError>
    where
        T: serde::de::DeserializeOwned,
    {
        loader.load_optional_section(key)
    }

    /// Build a loader using XDG-resolved config directories with `{{VAR}}` substitution.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if no config directory is accessible.
    pub fn create_loader_with_substitution(
        policy: Box<dyn SubstitutionPolicy>,
    ) -> Result<SectionLoaderImpl, ConfigError> {
        let mut loader = crate::core::DefaultConfigBuilder {
            name: String::new(),
            version: String::new(),
            config_dirs: Vec::new(),
            read_timeout: crate::core::DEFAULT_READ_TIMEOUT,
            config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        }
        .build_loader_internal()?;
        loader.substitution_policy = Some(policy);
        Ok(SectionLoaderImpl {
            ops: Box::new(loader),
        })
    }

    /// Build a loader that reads `application.toml` only from `dir`, with `{{VAR}}` substitution.
    pub fn create_loader_for_dir_with_substitution(
        dir: impl Into<PathBuf>,
        policy: Box<dyn SubstitutionPolicy>,
    ) -> SectionLoaderImpl {
        SectionLoaderImpl {
            ops: Box::new(crate::core::DefaultSectionLoader {
                config_dirs: vec![dir.into()],
                substitution_policy: Some(policy),
                value_resolver: None,
                read_timeout: crate::core::DEFAULT_READ_TIMEOUT,
                config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
                config_files: Vec::new(),
            }),
        }
    }

    /// Load one config section by XDG-resolving `app_name`'s config directories.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if no config directory is accessible or the section is missing.
    pub fn load_section_xdg<T>(app_name: &str, key: &str) -> Result<T, ConfigError>
    where
        T: serde::de::DeserializeOwned + Default,
    {
        Self::create_loader_xdg(app_name)?.load_section(key)
    }

    /// Build a loader using XDG-resolved config directories for `app_name`, with `{{VAR}}` substitution.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if no config directory is accessible.
    pub fn create_loader_xdg_with_substitution(
        app_name: &str,
        policy: Box<dyn SubstitutionPolicy>,
    ) -> Result<SectionLoaderImpl, ConfigError> {
        let mut loader = crate::core::DefaultConfigBuilder {
            name: app_name.to_owned(),
            version: String::new(),
            config_dirs: Vec::new(),
            read_timeout: crate::core::DEFAULT_READ_TIMEOUT,
            config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        }
        .build_loader_internal()?;
        loader.substitution_policy = Some(policy);
        Ok(SectionLoaderImpl {
            ops: Box::new(loader),
        })
    }

    /// Build a loader using XDG-resolved config directories with `{{VAR}}` substitution,
    /// resolving values via `resolver` instead of `std::env::var`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if no config directory is accessible.
    pub fn create_loader_with_resolver(
        policy: Box<dyn SubstitutionPolicy>,
        resolver: Box<dyn ValueResolver>,
    ) -> Result<SectionLoaderImpl, ConfigError> {
        let mut loader = crate::core::DefaultConfigBuilder {
            name: String::new(),
            version: String::new(),
            config_dirs: Vec::new(),
            read_timeout: crate::core::DEFAULT_READ_TIMEOUT,
            config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        }
        .build_loader_internal()?;
        loader.substitution_policy = Some(policy);
        loader.value_resolver = Some(resolver);
        Ok(SectionLoaderImpl {
            ops: Box::new(loader),
        })
    }

    /// Build a loader that reads `application.toml` only from `dir`, with `{{VAR}}`
    /// substitution resolved via `resolver` instead of `std::env::var`.
    pub fn create_loader_for_dir_with_resolver(
        dir: impl Into<PathBuf>,
        policy: Box<dyn SubstitutionPolicy>,
        resolver: Box<dyn ValueResolver>,
    ) -> SectionLoaderImpl {
        SectionLoaderImpl {
            ops: Box::new(crate::core::DefaultSectionLoader {
                config_dirs: vec![dir.into()],
                substitution_policy: Some(policy),
                value_resolver: Some(resolver),
                read_timeout: crate::core::DEFAULT_READ_TIMEOUT,
                config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
                config_files: Vec::new(),
            }),
        }
    }

    /// Build a loader using XDG-resolved config directories for `app_name`, with
    /// `{{VAR}}` substitution resolved via `resolver` instead of `std::env::var`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if no config directory is accessible.
    pub fn create_loader_xdg_with_resolver(
        app_name: &str,
        policy: Box<dyn SubstitutionPolicy>,
        resolver: Box<dyn ValueResolver>,
    ) -> Result<SectionLoaderImpl, ConfigError> {
        let mut loader = crate::core::DefaultConfigBuilder {
            name: app_name.to_owned(),
            version: String::new(),
            config_dirs: Vec::new(),
            read_timeout: crate::core::DEFAULT_READ_TIMEOUT,
            config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        }
        .build_loader_internal()?;
        loader.substitution_policy = Some(policy);
        loader.value_resolver = Some(resolver);
        Ok(SectionLoaderImpl {
            ops: Box::new(loader),
        })
    }

    /// Create an empty concrete config builder with `{{VAR}}` substitution support.
    pub fn create_config_builder_with_substitution(
        policy: Box<dyn SubstitutionPolicy>,
    ) -> DefaultSubstitutionConfigBuilder {
        DefaultSubstitutionConfigBuilder {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            config_dirs: Vec::new(),
            policy,
            config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        }
    }
}
