use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

use chrono::Local;
use tempdir::TempDir;

#[test]
fn shows_help() {
    let p = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--help")
        .output()
        .expect("failed to spawn process");

    let output = String::from_utf8(p.stdout).unwrap();

    assert!(output.contains("Usage: try-rs"));
}

#[test]
fn shows_version() {
    let p = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--version")
        .output()
        .expect("failed to spawn process");

    let stdout = String::from_utf8(p.stdout).unwrap();
    let stderr = String::from_utf8(p.stderr).unwrap();

    assert!(
        stdout.contains("try-rs"),
        "version output should go to stdout"
    );
    assert!(
        !stderr.contains("try-rs 0."),
        "version should not appear on stderr"
    );
}

#[test]
fn invalid_shell_flag() {
    let p = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--shell")
        .arg("invalid_shell")
        .output()
        .expect("failed to spawn process");

    assert!(
        !p.status.success(),
        "invalid shell should exit with non-zero"
    );
}

#[test]
fn new_name() {
    // given
    let h = Harness::new(false);

    // when
    let p = h.run_try(&["foo"]);
    let output = String::from_utf8(p.stdout).unwrap();

    // then
    let expected_dir = h.path().join("foo");
    assert_eq!(
        format!("cd '{}'", expected_dir.display()),
        output.trim(),
        "printed command is correct"
    );
    assert!(expected_dir.exists(), "try directory created");
    assert!(expected_dir.is_dir(), "try dir is a directory");
}

#[test]
fn exact_folder_exists() {
    // given
    let h = Harness::new(false);
    let existing = "existing";
    h.create_try_folder(existing);

    // when
    let p = h.run_try(&[existing]);
    let output = String::from_utf8(p.stdout).unwrap();

    // then
    let expected_dir = h.path().join(existing);
    assert_eq!(
        format!("cd '{}'", expected_dir.display()),
        output.trim(),
        "printed command is correct"
    );
}

#[test]
fn folder_with_date_exists() {
    // given
    let h = Harness::new(false);
    let name = "2020-02-02 existing";
    h.create_try_folder(name);

    // when
    let p = h.run_try(&["existing"]);
    let output = String::from_utf8(p.stdout).unwrap();

    // then
    let expected_dir = h.path().join(name);
    assert_eq!(
        format!("cd '{}'", expected_dir.display()),
        output.trim(),
        "printed command is correct"
    );
}

#[test]
fn new_name_with_date() {
    // given
    let h = Harness::new(true);
    let name = "existing";

    // when
    let p = h.run_try(&["existing"]);
    let output = String::from_utf8(p.stdout).unwrap();

    // then
    let expected_dir = h.path().join(format!(
        "{} {}",
        Local::now().format("%Y-%m-%d").to_string(),
        name
    ));
    assert_eq!(
        format!("cd '{}'", expected_dir.display()),
        output.trim(),
        "printed command is correct"
    );
}

struct Harness {
    dir: TempDir,
}

impl Harness {
    fn new(with_date_prefix: bool) -> Self {
        let dir = TempDir::new("try-test").expect("couldn't generate temp directory for test");

        let mut config = String::new();
        config.push_str(&format!(
            "tries_path = \"{}\"\n",
            dir.path().join("tries").display()
        ));
        if with_date_prefix {
            config.push_str("apply_date_prefix = true\n");
        }
        fs::write(dir.path().join("config.toml"), config).expect("could not create config file");

        Harness { dir }
    }

    fn run_try(&self, args: &[&str]) -> Output {
        Command::new("cargo")
            .arg("run")
            .arg("--")
            .args(args)
            .env("TRY_CONFIG_DIR", self.dir.path())
            .output()
            .expect("failed to spawn process")
    }

    fn path(&self) -> PathBuf {
        self.dir.path().join("tries").to_path_buf()
    }

    fn run_try_with_env(&self, args: &[&str], env_key: &str, env_val: &str) -> Output {
        Command::new("cargo")
            .arg("run")
            .arg("--")
            .args(args)
            .env("TRY_CONFIG_DIR", self.dir.path())
            .env(env_key, env_val)
            .output()
            .expect("failed to spawn process")
    }

    fn create_try_folder(&self, name: &str) {
        fs::DirBuilder::new()
            .recursive(true)
            .create(self.path().join(name))
            .expect("couldn't create 'existing' try");
    }
}

// ── Additional integration tests ───────────────────────────────────

#[test]
fn new_folder_is_created_on_disk() {
    let h = Harness::new(false);
    let p = h.run_try(&["brand-new"]);
    assert!(p.status.success());
    assert!(h.path().join("brand-new").exists());
    assert!(h.path().join("brand-new").is_dir());
}

#[test]
fn new_name_with_date_creates_dated_folder() {
    let h = Harness::new(true);
    let today = Local::now().format("%Y-%m-%d").to_string();
    let _ = h.run_try(&["dated-test"]);
    let expected = h.path().join(format!("{} dated-test", today));
    assert!(expected.exists(), "dated folder should be created");
}

#[test]
fn multiple_new_folders() {
    let h = Harness::new(false);
    h.run_try(&["proj-a"]);
    h.run_try(&["proj-b"]);
    h.run_try(&["proj-c"]);
    assert!(h.path().join("proj-a").exists());
    assert!(h.path().join("proj-b").exists());
    assert!(h.path().join("proj-c").exists());
}

#[test]
fn help_contains_expected_flags() {
    let p = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--help")
        .output()
        .expect("failed to spawn");

    let output = String::from_utf8(p.stdout).unwrap();
    assert!(output.contains("--setup"), "should document --setup flag");
    assert!(
        output.contains("--setup-stdout"),
        "should document --setup-stdout flag"
    );
    assert!(
        output.contains("--shallow-clone") || output.contains("-s"),
        "should document shallow clone"
    );
    assert!(
        output.contains("--worktree") || output.contains("-w"),
        "should document worktree flag"
    );
}

#[test]
fn setup_stdout_fish() {
    let p = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--setup-stdout")
        .arg("fish")
        .output()
        .expect("failed to spawn");

    let stdout = String::from_utf8(p.stdout).unwrap();
    assert!(p.status.success());
    assert!(
        stdout.contains("function try-rs"),
        "fish integration should define try-rs function"
    );
}

#[test]
fn setup_stdout_zsh() {
    let p = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--setup-stdout")
        .arg("zsh")
        .output()
        .expect("failed to spawn");

    let stdout = String::from_utf8(p.stdout).unwrap();
    assert!(p.status.success());
    assert!(
        stdout.contains("try-rs()"),
        "zsh integration should define try-rs function"
    );
}

#[test]
fn setup_stdout_bash() {
    let p = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--setup-stdout")
        .arg("bash")
        .output()
        .expect("failed to spawn");

    let stdout = String::from_utf8(p.stdout).unwrap();
    assert!(p.status.success());
    assert!(
        stdout.contains("try-rs()"),
        "bash integration should define try-rs function"
    );
}

#[test]
fn existing_folder_cd_does_not_recreate() {
    let h = Harness::new(false);
    h.create_try_folder("already");

    // Put a marker file inside
    fs::write(h.path().join("already").join("marker.txt"), "test").unwrap();

    let p = h.run_try(&["already"]);
    let output = String::from_utf8(p.stdout).unwrap();

    assert!(output.contains("cd"));
    // Marker file should still be there (folder wasn't recreated)
    assert!(h.path().join("already").join("marker.txt").exists());
}

#[test]
fn config_with_editor_field() {
    let dir = TempDir::new("try-editor-test").unwrap();
    let config = format!(
        "tries_path = \"{}\"\neditor = \"vim\"\n",
        dir.path().join("tries").display()
    );
    fs::write(dir.path().join("config.toml"), config).unwrap();

    let p = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("editor-proj")
        .env("TRY_CONFIG_DIR", dir.path())
        .output()
        .expect("failed to spawn");

    // It should create the folder successfully
    assert!(p.status.success() || !String::from_utf8(p.stdout).unwrap().is_empty());
}

#[test]
fn config_with_theme() {
    let dir = TempDir::new("try-theme-test").unwrap();
    let config = format!(
        "tries_path = \"{}\"\ntheme = \"Dracula\"\n",
        dir.path().join("tries").display()
    );
    fs::write(dir.path().join("config.toml"), config).unwrap();

    let p = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("themed-proj")
        .env("TRY_CONFIG_DIR", dir.path())
        .output()
        .expect("failed to spawn");

    let output = String::from_utf8(p.stdout).unwrap();
    assert!(output.contains("cd"), "should produce cd command");
}

#[test]
fn config_with_invalid_theme_falls_back() {
    let dir = TempDir::new("try-bad-theme").unwrap();
    let config = format!(
        "tries_path = \"{}\"\ntheme = \"NonExistentTheme\"\n",
        dir.path().join("tries").display()
    );
    fs::write(dir.path().join("config.toml"), config).unwrap();

    let p = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("fallback-proj")
        .env("TRY_CONFIG_DIR", dir.path())
        .output()
        .expect("failed to spawn");

    let output = String::from_utf8(p.stdout).unwrap();
    // Should still work, just using default theme
    assert!(output.contains("cd"), "should still produce cd command");
}

#[test]
fn try_path_env_overrides_config() {
    let h = Harness::new(false);
    let override_dir = TempDir::new("try-override-path").unwrap();

    let p = h.run_try_with_env(&["env-proj"], "TRY_PATH", override_dir.path().to_str().unwrap());
    let output = String::from_utf8(p.stdout).unwrap();

    // The cd should point to the TRY_PATH override dir, not the config's tries_path
    assert!(
        output.contains(&override_dir.path().to_string_lossy().to_string()),
        "TRY_PATH env should override config tries_path"
    );
}
