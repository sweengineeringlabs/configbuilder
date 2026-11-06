//! Public concrete config builder returned by `create_config_builder`.

use std::path::PathBuf;
use std::time::Duration;

/// Concrete config builder returned by [`ConfigLoaderFactory::create_config_builder`](crate::ConfigLoaderFactory::create_config_builder).
///
/// This is the **only type from which you can call `build_loader()`** to finalise
/// configuration into a [`SectionLoaderImpl`].  The `build_loader` method is an
/// inherent method added by an extension impl in `saf/` (not on the builder
/// trait) so that this declaration in `api/` carries no dependency on `core/`.
///
/// Chain the fluent setters to configure XDG resolution, then call `build_loader()`
/// to get a [`SectionLoaderImpl`] ready to call `load_section` on.
///
/// # Why not `impl ConfigBuilder`?
///
/// SAF `create_config_builder()` functions return this concrete type, not
/// `impl ConfigBuilder`.  Returning the opaque trait type would prevent callers
/// from ever calling `build_loader()`, because `build_loader` is not part of the
/// builder trait contract.
///
/// [`ConfigLoaderFactory::create_config_builder`]: crate::ConfigLoaderFactory::create_config_builder
/// [`SectionLoaderImpl`]: crate::SectionLoaderImpl
///
/// # Examples
///
/// ```rust,no_run
/// use configbuilder::{
///     BuilderFinalizer as _, ConfigBuilder as _, ConfigBuilderImpl, ConfigBuilderInit as _,
///     Loader as _,
/// };
///
/// #[derive(serde::Deserialize, Default)]
/// struct AuthConfig { token_ttl_secs: u64 }
///
/// let loader = ConfigBuilderImpl::new()
///     .with_name(env!("CARGO_PKG_NAME"))
///     .with_version(env!("CARGO_PKG_VERSION"))
///     .with_config_dir("config/")
///     .build_loader()
///     .expect("config dir must be readable");
///
/// let cfg: AuthConfig = loader.load_section("auth").expect("auth section required");
/// ```
pub struct ConfigBuilderImpl {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) config_dirs: Vec<PathBuf>,
    pub(crate) read_timeout: Option<Duration>,
    pub(crate) config_filename: Option<String>,
    pub(crate) config_files: Vec<PathBuf>,
}
