//! `ConfigBuilder::with_config_file`: a file of any name is a source of the chain, read after every directory.

use configbuilder::{
    BuilderFinalizer as _, ConfigBuilder as _, ConfigError, ConfigLoaderFactory, Loader as _,
};

#[derive(Debug, Default, serde::Deserialize, PartialEq)]
#[serde(default)]
struct Section {
    value: String,
    count: u32,
}

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected Ok, got {error:?}"),
    }
}

fn write(directory: &std::path::Path, name: &str, text: &str) -> std::path::PathBuf {
    let path = directory.join(name);
    must(std::fs::write(&path, text));
    path
}

/// An application name no machine has a configuration directory for.
const UNIQUE_NAME: &str = "configbuilder-file-source-test-8c1d";

/// @covers: with_config_file
#[test]
fn test_with_config_file_file_of_another_name_overrides_the_directories_happy() {
    let dir = must(tempfile::tempdir());
    write(
        dir.path(),
        "application.toml",
        "[s]\nvalue = \"from the directory\"\ncount = 7\n",
    );
    let file = write(dir.path(), "prod.toml", "[s]\nvalue = \"from the file\"\n");
    let loader = must(
        ConfigLoaderFactory::create_config_builder()
            .with_config_dir(dir.path())
            .with_config_file(&file)
            .build_loader(),
    );
    let section: Section = must(loader.load_section("s"));
    assert_eq!(
        section.value, "from the file",
        "the file is read after the directory and wins"
    );
    assert_eq!(
        section.count, 7,
        "a key the file does not set keeps the value of the directory"
    );
}

/// @covers: with_config_file
#[test]
fn test_with_config_file_later_file_wins_over_an_earlier_one_edge() {
    let dir = must(tempfile::tempdir());
    let first = write(dir.path(), "a.toml", "[s]\nvalue = \"first\"\ncount = 1\n");
    let second = write(dir.path(), "b.toml", "[s]\nvalue = \"second\"\n");
    let loader = must(
        ConfigLoaderFactory::create_config_builder()
            .with_config_dir(dir.path())
            .with_config_file(&first)
            .with_config_file(&second)
            .build_loader(),
    );
    let section: Section = must(loader.load_section("s"));
    assert_eq!((section.value.as_str(), section.count), ("second", 1));
}

/// @covers: with_config_file
#[test]
fn test_with_config_file_file_alone_is_the_only_source_of_a_named_application_edge() {
    let dir = must(tempfile::tempdir());
    let file = write(dir.path(), "only.toml", "[s]\nvalue = \"only\"\n");
    let loader = must(
        ConfigLoaderFactory::create_config_builder()
            .with_name(UNIQUE_NAME)
            .with_config_file(&file)
            .build_loader(),
    );
    let section: Section = must(loader.load_section("s"));
    assert_eq!(section.value, "only");
}

/// @covers: with_config_file
#[test]
fn test_with_config_file_file_that_does_not_exist_is_not_found_error() {
    let dir = must(tempfile::tempdir());
    write(dir.path(), "application.toml", "[s]\nvalue = \"dir\"\n");
    let missing = dir.path().join("missing.toml");
    let loader = must(
        ConfigLoaderFactory::create_config_builder()
            .with_config_dir(dir.path())
            .with_config_file(&missing)
            .build_loader(),
    );
    let outcome: Result<Section, ConfigError> = loader.load_section("s");
    assert!(
        matches!(outcome, Err(ConfigError::NotFound(_))),
        "a file the caller named must exist, even when a directory has a value: {outcome:?}"
    );
}

/// @covers: with_config_file
#[test]
fn test_with_config_file_file_that_is_not_valid_toml_is_a_parse_error_error() {
    let dir = must(tempfile::tempdir());
    let file = write(dir.path(), "broken.toml", "[s\nvalue = ");
    let loader = must(
        ConfigLoaderFactory::create_config_builder()
            .with_config_dir(dir.path())
            .with_config_file(&file)
            .build_loader(),
    );
    let outcome: Result<Section, ConfigError> = loader.load_section("s");
    assert!(matches!(outcome, Err(ConfigError::Parse(_))), "{outcome:?}");
}

/// @covers: with_config_file
#[test]
fn test_with_config_file_directory_given_as_a_file_is_refused_when_the_loader_is_built_error() {
    let dir = must(tempfile::tempdir());
    let outcome = ConfigLoaderFactory::create_config_builder()
        .with_config_dir(dir.path())
        .with_config_file(dir.path())
        .build_loader();
    assert!(
        matches!(outcome, Err(ConfigError::Io(_))),
        "a directory is not a config file"
    );
}

/// @covers: with_config_file
#[test]
fn test_with_config_file_file_over_the_size_limit_is_an_io_error_error() {
    let dir = must(tempfile::tempdir());
    let big = format!("[s]\nvalue = \"{}\"\n", "x".repeat(1024 * 1024 + 1));
    let file = write(dir.path(), "big.toml", &big);
    let loader = must(
        ConfigLoaderFactory::create_config_builder()
            .with_config_dir(dir.path())
            .with_config_file(&file)
            .build_loader(),
    );
    let outcome: Result<Section, ConfigError> = loader.load_section("s");
    assert!(
        matches!(outcome, Err(ConfigError::Io(_))),
        "the limit of a directory's file applies to a named file: {outcome:?}"
    );
}

/// @covers: with_config_file
#[test]
fn test_with_config_file_file_without_the_section_leaves_the_directories_value_edge() {
    let dir = must(tempfile::tempdir());
    write(dir.path(), "application.toml", "[s]\nvalue = \"dir\"\n");
    let file = write(dir.path(), "other.toml", "[unrelated]\nkey = 1\n");
    let loader = must(
        ConfigLoaderFactory::create_config_builder()
            .with_config_dir(dir.path())
            .with_config_file(&file)
            .build_loader(),
    );
    let section: Section = must(loader.load_section("s"));
    assert_eq!(section.value, "dir");
}
