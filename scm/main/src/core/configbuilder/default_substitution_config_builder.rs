//! Public concrete config builder returned by
//! `create_config_builder_with_substitution`.

use std::path::PathBuf;

use crate::api::{
    ConfigBuilder, ConfigBuilderImpl, ConfigError, SectionLoaderImpl, SubstitutionPolicy,
};

/// A ready-to-use config builder with substitution support, produced by
/// [`ConfigLoaderFactory::create_config_builder_with_substitution`].
///
/// Use the fluent builder methods to configure directories, then call
/// `build_loader` to obtain a [`SectionLoaderImpl`] that will expand `{{VAR}}`
/// placeholders in TOML values using the bound [`SubstitutionPolicy`].
///
/// [`ConfigLoaderFactory::create_config_builder_with_substitution`]: crate::ConfigLoaderFactory::create_config_builder_with_substitution
/// [`SectionLoaderImpl`]: crate::SectionLoaderImpl
/// [`SubstitutionPolicy`]: crate::SubstitutionPolicy
///
/// # Examples
///
/// ```rust,no_run
/// use configbuilder::{BuilderFinalizer as _, ConfigBuilder as _, ConfigLoaderFactory, Loader as _};
///
/// #[derive(serde::Deserialize, Default)]
/// struct DbConfig { url: String }
///
/// // TOML: url = "postgres://{{DB_USER}}:{{DB_PASS}}@host/db"
/// let loader = ConfigLoaderFactory::create_config_builder_with_substitution(
///         Box::new(ConfigLoaderFactory::create_prefix_whitelist_policy(vec![
///             "APP_".to_string()
///         ])),
///     )
///     .with_config_dir("config/")
///     .build_loader()
///     .expect("config dir accessible");
///
/// let cfg: DbConfig = loader.load_section("database").expect("database section required");
/// // cfg.url has had {{DB_USER}} and {{DB_PASS}} substituted.
/// ```
pub struct DefaultSubstitutionConfigBuilder {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) config_dirs: Vec<PathBuf>,
    pub(crate) policy: Box<dyn SubstitutionPolicy>,
    pub(crate) config_filename: String,
    pub(crate) config_files: Vec<PathBuf>,
}

impl DefaultSubstitutionConfigBuilder {
    /// Return the configured application name.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Return the configured application version.
    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    /// Set the application name.
    pub(crate) fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the application version string.
    pub(crate) fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Append an explicit config directory.
    pub(crate) fn with_config_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config_dirs.push(dir.into());
        self
    }

    /// Override the config filename searched for in each configured directory.
    pub(crate) fn with_config_filename(mut self, filename: impl Into<String>) -> Self {
        self.config_filename = filename.into();
        self
    }

    /// Append one explicit config file, of any name, read after every directory.
    pub(crate) fn with_config_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.config_files.push(file.into());
        self
    }

    /// Consume the builder and return a ready-to-use section loader with substitution support.
    pub(crate) fn build_loader(self) -> Result<SectionLoaderImpl, ConfigError> {
        let mut core = crate::core::DefaultConfigBuilder {
            name: self.name,
            version: self.version,
            config_dirs: self.config_dirs,
            read_timeout: crate::core::loader::DEFAULT_READ_TIMEOUT,
            config_filename: self.config_filename,
            config_files: self.config_files,
        }
        .build_loader_internal()?;
        core.substitution_policy = Some(self.policy);
        Ok(SectionLoaderImpl {
            ops: Box::new(core),
        })
    }
}

impl crate::api::BuilderFinalizer for DefaultSubstitutionConfigBuilder {
    type Loader = SectionLoaderImpl;
    type Error = ConfigError;

    fn build_loader(self) -> Result<Self::Loader, Self::Error> {
        DefaultSubstitutionConfigBuilder::build_loader(self)
    }
}

impl ConfigBuilder for DefaultSubstitutionConfigBuilder {
    fn name(&self) -> &str {
        DefaultSubstitutionConfigBuilder::name(self)
    }

    fn version(&self) -> &str {
        DefaultSubstitutionConfigBuilder::version(self)
    }

    fn with_name(self, name: impl Into<String>) -> Self {
        DefaultSubstitutionConfigBuilder::with_name(self, name)
    }

    fn with_version(self, version: impl Into<String>) -> Self {
        DefaultSubstitutionConfigBuilder::with_version(self, version)
    }

    fn with_config_dir(self, dir: impl Into<std::path::PathBuf>) -> Self {
        DefaultSubstitutionConfigBuilder::with_config_dir(self, dir)
    }

    fn with_config_filename(self, filename: impl Into<String>) -> Self {
        DefaultSubstitutionConfigBuilder::with_config_filename(self, filename)
    }

    fn with_config_file(self, file: impl Into<std::path::PathBuf>) -> Self {
        DefaultSubstitutionConfigBuilder::with_config_file(self, file)
    }
}

impl From<DefaultSubstitutionConfigBuilder> for ConfigBuilderImpl {
    fn from(value: DefaultSubstitutionConfigBuilder) -> Self {
        ConfigBuilderImpl {
            name: value.name,
            version: value.version,
            config_dirs: value.config_dirs,
            read_timeout: None,
            config_filename: Some(value.config_filename),
            config_files: value.config_files,
        }
    }
}
