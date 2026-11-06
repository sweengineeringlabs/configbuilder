//! `ConfigBuilder::with_config_file(file)` — appends one explicit config file, of any name, to the sources of the loader.
//!
//! ## When to use this
//!
//! `with_config_dir` reads `<dir>/<config_filename>`, and the filename is one value for the whole loader. Use `with_config_file` for a source that is a file with
//! its own name: for example the path an operator gives in an environment variable (`/etc/app/prod.toml`). Files are read after every directory, in the
//! order added, so they override the directories. Unlike a directory, a file named here must exist: the loader reports `ConfigError::Io` when it does not, and keeps `ConfigError::NotFound` for a section that is in no source.
//!
//! ## What this example does
//!
//! 1. Makes a directory with an `application.toml` that sets `value` and `count`, and a file `prod.toml` that sets `value` only.
//! 2. Chains `with_config_dir(dir)` then `with_config_file(prod.toml)`.
//! 3. Loads the section and asserts that `value` comes from the file and `count` from the directory.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use configbuilder::{BuilderFinalizer as _, ConfigBuilder as _, ConfigLoaderFactory, Loader as _};

#[derive(Debug, Default, serde::Deserialize, PartialEq)]
#[serde(default)]
struct AppConfig {
    value: String,
    count: u32,
}

fn main() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("application.toml"),
        "[app]\nvalue = \"from the directory\"\ncount = 7\n",
    )
    .expect("write application.toml");
    let file = dir.path().join("prod.toml");
    std::fs::write(&file, "[app]\nvalue = \"from the file\"\n").expect("write prod.toml");

    let loader = ConfigLoaderFactory::create_config_builder()
        .with_config_dir(dir.path())
        .with_config_file(&file)
        .build_loader()
        .expect("build_loader");
    let cfg: AppConfig = loader.load_section("app").expect("load app section");

    assert_eq!(cfg.value, "from the file");
    assert_eq!(cfg.count, 7);
    println!(
        "with_config_file: the file wins on value={}, the directory keeps count={}",
        cfg.value, cfg.count
    );
}
