use std::path::PathBuf;

use tempdir::TempDir;
use try_rs::config::*;
use try_rs::themes::Theme;

#[test]
fn save_and_reload_config() {
    let tmp = TempDir::new("save-config-test").unwrap();
    let config_path = tmp.path().join("config.toml");
    let theme = Theme::default();
    let tries_path = PathBuf::from("/tmp/tries");

    save_config(
        &config_path,
        &theme,
        &tries_path,
        &Some("code".to_string()),
        Some(true),
        Some(false),
    )
    .unwrap();

    let contents = std::fs::read_to_string(&config_path).unwrap();
    let loaded: Config = toml::from_str(&contents).unwrap();
    assert_eq!(loaded.tries_path.as_deref(), Some("/tmp/tries"));
    assert_eq!(loaded.theme.as_deref(), Some("Default"));
    assert_eq!(loaded.editor.as_deref(), Some("code"));
    assert_eq!(loaded.apply_date_prefix, Some(true));
    assert_eq!(loaded.transparent_background, Some(false));
}

#[test]
fn save_config_creates_parent_dirs() {
    let tmp = TempDir::new("save-nested").unwrap();
    let config_path = tmp.path().join("nested").join("dir").join("config.toml");
    let theme = Theme::default();

    save_config(
        &config_path,
        &theme,
        &PathBuf::from("/tmp/t"),
        &None,
        None,
        None,
    )
    .unwrap();

    assert!(config_path.exists());
}

#[test]
fn save_config_none_optionals() {
    let tmp = TempDir::new("save-none").unwrap();
    let config_path = tmp.path().join("config.toml");
    let theme = Theme::default();

    save_config(
        &config_path,
        &theme,
        &PathBuf::from("/tmp/t"),
        &None,
        None,
        None,
    )
    .unwrap();

    let contents = std::fs::read_to_string(&config_path).unwrap();
    let loaded: Config = toml::from_str(&contents).unwrap();
    assert!(loaded.editor.is_none());
    assert!(loaded.apply_date_prefix.is_none());
    assert!(loaded.transparent_background.is_none());
}

#[test]
fn config_serialization_roundtrip() {
    let config = Config {
        tries_path: Some("~/work/tries".to_string()),
        theme: Some("Tokyo Night".to_string()),
        editor: Some("nvim".to_string()),
        apply_date_prefix: Some(true),
        transparent_background: Some(true),
    };

    let toml_str = toml::to_string(&config).unwrap();
    let loaded: Config = toml::from_str(&toml_str).unwrap();

    assert_eq!(loaded.tries_path, config.tries_path);
    assert_eq!(loaded.theme, config.theme);
    assert_eq!(loaded.editor, config.editor);
    assert_eq!(loaded.apply_date_prefix, config.apply_date_prefix);
    assert_eq!(loaded.transparent_background, config.transparent_background);
}

#[test]
fn config_deserialize_empty() {
    let config: Config = toml::from_str("").unwrap();
    assert!(config.tries_path.is_none());
    assert!(config.theme.is_none());
    assert!(config.editor.is_none());
    assert!(config.apply_date_prefix.is_none());
    assert!(config.transparent_background.is_none());
}

#[test]
fn config_deserialize_partial() {
    let config: Config = toml::from_str(r#"theme = "Nord""#).unwrap();
    assert_eq!(config.theme.as_deref(), Some("Nord"));
    assert!(config.tries_path.is_none());
}

#[test]
fn config_ignores_unknown_fields() {
    let result: Result<Config, _> = toml::from_str(
        r#"
theme = "Default"
unknown_field = "value"
"#,
    );
    let _ = result;
}

#[test]
fn get_file_config_toml_name_default() {
    unsafe { std::env::remove_var("TRY_CONFIG") };
    assert_eq!(get_file_config_toml_name(), "config.toml");
}

#[test]
fn get_config_dir_with_env() {
    unsafe { std::env::set_var("TRY_CONFIG_DIR", "/custom/config/dir") };
    let dir = get_config_dir();
    assert_eq!(dir, PathBuf::from("/custom/config/dir"));
    unsafe { std::env::remove_var("TRY_CONFIG_DIR") };
}

#[test]
fn save_config_preserves_theme_name() {
    let tmp = TempDir::new("theme-name").unwrap();
    let config_path = tmp.path().join("config.toml");

    let themes = Theme::all();
    for theme in &themes {
        save_config(
            &config_path,
            theme,
            &PathBuf::from("/tmp/t"),
            &None,
            None,
            None,
        )
        .unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let loaded: Config = toml::from_str(&contents).unwrap();
        assert_eq!(loaded.theme.as_deref(), Some(theme.name.as_str()));
    }
}
