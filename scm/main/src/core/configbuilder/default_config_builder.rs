//! `DefaultConfigBuilder` — default `ConfigBuilder` implementation.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::api::{ConfigBuilder, ConfigBuilderBound, ConfigError, Loader as _};
use crate::core::DefaultSectionLoader;

const CONFIG_DIR_ENV_VAR: &str = "CONFIGBUILDER_CONFIG_DIR";
const FALLBACK_CONFIG_DIR: &str = "config";

pub(crate) struct DefaultConfigBuilder {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) config_dirs: Vec<PathBuf>,
    pub(crate) read_timeout: Duration,
    pub(crate) config_filename: String,
    pub(crate) config_files: Vec<PathBuf>,
}

impl DefaultConfigBuilder {
    fn reject_traversal(path: &Path) -> Result<(), ConfigError> {
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(ConfigError::Io(format!(
                "{}: path traversal via '..' is not permitted",
                path.display()
            )));
        }
        Ok(())
    }
}

impl DefaultConfigBuilder {
    pub(crate) fn build_loader_internal(self) -> Result<DefaultSectionLoader, ConfigError> {
        if !self.config_dirs.is_empty() {
            let loader = DefaultSectionLoader {
                config_dirs: self.config_dirs,
                substitution_policy: None,
                value_resolver: None,
                read_timeout: self.read_timeout,
                config_filename: self.config_filename,
                config_files: self.config_files,
            };
            loader.validate()?;
            return Ok(loader);
        }

        if !self.name.is_empty() {
            let mut dirs: Vec<PathBuf> = Vec::new();
            let xdg_config_dirs =
                env::var("XDG_CONFIG_DIRS").unwrap_or_else(|_| "/etc/xdg".to_owned());
            for segment in xdg_config_dirs.split(':').rev() {
                if !segment.is_empty() {
                    let seg_path = PathBuf::from(segment);
                    Self::reject_traversal(&seg_path)?;
                    dirs.push(seg_path.join(&self.name));
                }
            }
            if let Some(home) = dirs::config_dir() {
                dirs.push(home.join(&self.name));
            }
            if let Ok(v) = env::var(CONFIG_DIR_ENV_VAR) {
                let p = PathBuf::from(&v);
                Self::reject_traversal(&p)?;
                dirs.push(p);
            }
            let loader = DefaultSectionLoader {
                config_dirs: dirs,
                substitution_policy: None,
                value_resolver: None,
                read_timeout: self.read_timeout,
                config_filename: self.config_filename,
                config_files: self.config_files,
            };
            loader.validate()?;
            return Ok(loader);
        }

        let dir = match env::var(CONFIG_DIR_ENV_VAR) {
            Ok(v) => {
                let p = PathBuf::from(&v);
                Self::reject_traversal(&p)?;
                p
            }
            Err(_) => PathBuf::from(FALLBACK_CONFIG_DIR),
        };
        let loader = DefaultSectionLoader {
            config_dirs: vec![dir],
            substitution_policy: None,
            value_resolver: None,
            read_timeout: self.read_timeout,
            config_filename: self.config_filename,
            config_files: self.config_files,
        };
        loader.validate()?;
        Ok(loader)
    }
}

impl ConfigBuilder for DefaultConfigBuilder {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    fn with_config_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config_dirs.push(dir.into());
        self
    }

    fn with_config_filename(mut self, filename: impl Into<String>) -> Self {
        self.config_filename = filename.into();
        self
    }

    fn with_config_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.config_files.push(file.into());
        self
    }
}

impl ConfigBuilderBound for DefaultConfigBuilder {
    type ApplicationConfig = crate::api::ApplicationConfig;
    type Builder = crate::api::ConfigBuilderImpl;
    type SubstitutionBuilder = crate::core::DefaultSubstitutionConfigBuilder;
}

/// Builder-style accessors used only by this module's unit tests.
/// The production builder API lives on [`ConfigBuilderImpl`](crate::api::configbuilder::vo::ConfigBuilderImpl).
#[cfg(test)]
impl DefaultConfigBuilder {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub(crate) fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub(crate) fn with_config_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config_dirs.push(dir.into());
        self
    }

    pub(crate) fn with_config_filename(mut self, filename: impl Into<String>) -> Self {
        self.config_filename = filename.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ConfigError;
    use std::io::Write as _;

    fn must<T, E>(result: Result<T, E>) -> T {
        result.unwrap_or_else(|_| std::process::abort())
    }

    fn path_str(path: &Path) -> &str {
        match path.to_str() {
            Some(value) => value,
            None => std::process::abort(),
        }
    }

    fn blank() -> DefaultConfigBuilder {
        DefaultConfigBuilder {
            name: String::new(),
            version: "0.1.0".to_string(),
            config_dirs: Vec::new(),
            read_timeout: Duration::from_secs(30),
            config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        }
    }

    #[test]
    fn test_blank_builder_has_empty_name_and_default_version() {
        let b = blank();
        assert_eq!(b.name(), "");
        assert_eq!(b.version(), "0.1.0");
    }

    #[test]
    fn test_with_name_sets_application_name() {
        let b = blank().with_name("example-app-config");
        assert_eq!(b.name(), "example-app-config");
    }

    #[test]
    fn test_with_version_sets_application_version() {
        let b = blank().with_version("2.0.0");
        assert_eq!(b.version(), "2.0.0");
    }

    #[test]
    fn test_with_config_dir_appends_to_config_dirs() {
        let b = blank().with_config_dir("/some/path");
        assert_eq!(b.config_dirs, vec![PathBuf::from("/some/path")]);
    }

    #[test]
    fn test_with_config_dir_multiple_calls_accumulate() {
        let b = blank().with_config_dir("/a").with_config_dir("/b");
        assert_eq!(
            b.config_dirs,
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
    }

    #[test]
    fn test_with_config_filename_overrides_default() {
        let b = blank().with_config_filename("custom.toml");
        assert_eq!(b.config_filename, "custom.toml");
    }

    #[derive(Debug, Default, serde::Deserialize, PartialEq)]
    #[serde(default)]
    struct DefaultConfigBuilderFixture {
        value: String,
    }

    #[test]
    fn test_build_loader_with_explicit_dir_reads_application_toml() {
        let dir = must(tempfile::tempdir());
        let mut f = must(std::fs::File::create(dir.path().join("application.toml")));
        must(writeln!(f, "[svc]\nvalue = \"ok\""));
        let loader = must(blank().with_config_dir(dir.path()).build_loader_internal());
        let sec: DefaultConfigBuilderFixture = must(loader.load_section("svc"));
        assert_eq!(sec.value, "ok");
    }

    #[test]
    fn test_build_loader_with_custom_filename_ignores_application_toml() {
        let dir = must(tempfile::tempdir());
        // A stray application.toml must NOT be read once a custom filename is set.
        let mut wrong = must(std::fs::File::create(dir.path().join("application.toml")));
        must(writeln!(wrong, "[svc]\nvalue = \"wrong\""));
        let mut right = must(std::fs::File::create(dir.path().join("custom.toml")));
        must(writeln!(right, "[svc]\nvalue = \"right\""));

        let loader = must(
            blank()
                .with_config_dir(dir.path())
                .with_config_filename("custom.toml")
                .build_loader_internal(),
        );
        let sec: DefaultConfigBuilderFixture = must(loader.load_section("svc"));
        assert_eq!(sec.value, "right");
    }

    #[test]
    fn test_build_loader_with_unknown_name_returns_not_found() {
        let loader = must(
            blank()
                .with_name("example-app-nonexistent-test-xyz")
                .build_loader_internal(),
        );
        let result: Result<DefaultConfigBuilderFixture, _> = loader.load_section("any");
        assert!(
            matches!(result, Err(ConfigError::NotFound(_))),
            "expected NotFound for unknown app, got {result:?}"
        );
    }

    #[test]
    fn test_build_loader_no_name_no_dirs_no_application_toml_returns_not_found() {
        // Point CONFIGBUILDER_CONFIG_DIR to an empty temp dir so there is no
        // application.toml — load_section must return NotFound.
        let dir = must(tempfile::tempdir());
        std::env::set_var("CONFIGBUILDER_CONFIG_DIR", path_str(dir.path()));
        let loader = must(blank().build_loader_internal());
        let result: Result<DefaultConfigBuilderFixture, _> =
            loader.load_section("nonexistent_section_xyz");
        std::env::remove_var("CONFIGBUILDER_CONFIG_DIR");
        assert!(
            matches!(result, Err(ConfigError::NotFound(_))),
            "expected NotFound when no application.toml exists in config dir, got {result:?}"
        );
    }

    #[test]
    fn test_reject_traversal_rejects_dotdot_path() {
        assert!(
            DefaultConfigBuilder::reject_traversal(std::path::Path::new("../../etc")).is_err(),
            "expected Io error for '..' path"
        );
    }

    #[test]
    fn test_reject_traversal_accepts_absolute_path() {
        let path = std::path::Path::new("/etc/xdg/myapp");
        assert!(!path
            .components()
            .any(|c| { matches!(c, std::path::Component::ParentDir) }));
        assert!(matches!(
            DefaultConfigBuilder::reject_traversal(path),
            Ok(())
        ));
    }

    #[test]
    fn test_build_loader_internal() {
        let dir = must(tempfile::tempdir());
        let builder = DefaultConfigBuilder {
            name: String::new(),
            version: String::new(),
            config_dirs: vec![dir.path().to_path_buf()],
            read_timeout: Duration::from_secs(30),
            config_filename: crate::core::DEFAULT_CONFIG_FILENAME.to_string(),
            config_files: Vec::new(),
        };
        let loader = must(builder.build_loader_internal());
        assert_eq!(loader.config_dirs, vec![dir.path().to_path_buf()]);
        assert_eq!(loader.read_timeout, Duration::from_secs(30));
    }
}
