use anyhow::Result;

use clap::{Parser, ValueEnum};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use serde::Deserialize;
use std::process::Stdio;
use std::str::FromStr;
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

mod tui;

use tui::{App, Theme, run_app};

// Representation of our TOML file
#[derive(Deserialize)]
struct ThemeConfig {
    title_try: Option<String>,
    title_rs: Option<String>,
    search_box: Option<String>,
    list_date: Option<String>,
    list_highlight_bg: Option<String>,
    list_highlight_fg: Option<String>,
    help_text: Option<String>,
    status_message: Option<String>,
    popup_bg: Option<String>,
    popup_text: Option<String>,
}

#[derive(Deserialize)]
struct Config {
    tries_path: Option<String>,
    colors: Option<ThemeConfig>,
    editor: Option<String>,
}

// Helper function to replace "~" with the actual home path
fn expand_path(path_str: &str) -> PathBuf {
    if (path_str.starts_with("~/") || (cfg!(windows) && path_str.starts_with("~\\")))
        && let Some(home) = dirs::home_dir()
    {
        // Remove "~/" (first 2 chars) and join with home
        return home.join(&path_str[2..]);
    }
    PathBuf::from(path_str)
}

fn load_configuration() -> (PathBuf, Theme, Option<String>, bool) {
    // 1. Try to find the default config directory (~/.config)
    let config_dir = dirs::config_dir().unwrap_or_else(|| {
        // Fallback if not found
        dirs::home_dir().expect("Folder not found").join(".config")
    });

    // 2. Build the path: ~/.config/try-rs/config.toml
    let app_config_dir = config_dir.join("try-rs");
    let config_file = app_config_dir
        .join(std::env::var_os("TRY_CONFIG").unwrap_or_else(|| "config".into()))
        .with_extension("toml");

    // 3. Define the old default (fallback)
    let default_path = dirs::home_dir()
        .expect("Folder not found")
        .join("work")
        .join("tries");

    let mut theme = Theme::default();
    let try_path = std::env::var_os("TRY_PATH");
    let try_path_specified = try_path.is_some();
    let mut final_path = try_path.map(PathBuf::from).unwrap_or(default_path);
    let mut editor_cmd = std::env::var("VISUAL")
        .ok()
        .or_else(|| std::env::var("EDITOR").ok());
    let mut is_first_run = false;

    // 4. If the file exists, try to read it
    if config_file.exists() {
        if let Ok(contents) = fs::read_to_string(&config_file)
            && let Ok(config) = toml::from_str::<Config>(&contents)
        {
            if let Some(path_str) = config.tries_path
                && !try_path_specified
            {
                final_path = expand_path(&path_str);
            }
            if let Some(editor) = config.editor {
                editor_cmd = Some(editor);
            }
            if let Some(colors) = config.colors {
                // Helper to parse color string to Color enum
                let parse = |opt: Option<String>, def: Color| -> Color {
                    opt.and_then(|s| Color::from_str(&s).ok()).unwrap_or(def)
                };

                let def = Theme::default();
                theme = Theme {
                    title_try: parse(colors.title_try, def.title_try),
                    title_rs: parse(colors.title_rs, def.title_rs),
                    search_box: parse(colors.search_box, def.search_box),
                    list_date: parse(colors.list_date, def.list_date),
                    list_highlight_bg: parse(colors.list_highlight_bg, def.list_highlight_bg),
                    list_highlight_fg: parse(colors.list_highlight_fg, def.list_highlight_fg),
                    help_text: parse(colors.help_text, def.help_text),
                    status_message: parse(colors.status_message, def.status_message),
                    popup_bg: parse(colors.popup_bg, def.popup_bg),
                    popup_text: parse(colors.popup_text, def.popup_text),
                };
            }
        }
    } else {
        // Create default config if it doesn't exist
        if fs::create_dir_all(&app_config_dir).is_ok() {
            let default_content = format!("tries_path = {final_path:?}");
            let _ = fs::write(&config_file, default_content);
            is_first_run = true;
        }
    }

    // If nothing works or there is no config, return the default
    (final_path, theme, editor_cmd, is_first_run)
}

fn setup_fish() -> Result<()> {
    let config_dir = dirs::config_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .expect("Could not find home directory")
            .join(".config")
    });

    let fish_functions_dir = config_dir.join("fish").join("functions");

    if !fish_functions_dir.exists() {
        fs::create_dir_all(&fish_functions_dir)?;
    }

    let file_path = fish_functions_dir.join("try-rs.fish");
    let content = r#"function try-rs
    # Captures the output of the binary (stdout) which is the "cd" command
    # The TUI is rendered on stderr, so it doesn't interfere.
    set command (command try-rs $argv | string collect)

    if test -n "$command"
        eval $command
    end
end
"#;

    fs::write(&file_path, content)?;
    eprintln!("Fish function created at: {}", file_path.display());
    eprintln!(
        "You may need to restart your shell or run 'source {}' to apply changes.",
        file_path.display()
    );

    Ok(())
}

fn setup_zsh() -> Result<()> {
    let config_dir = dirs::config_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .expect("Could not find home directory")
            .join(".config")
    });

    let app_config_dir = config_dir.join("try-rs");

    if !app_config_dir.exists() {
        fs::create_dir_all(&app_config_dir)?;
    }

    let file_path = app_config_dir.join("try-rs.zsh");
    let content = r#"try-rs() {
    # Captures the output of the binary (stdout) which is the "cd" command
    # The TUI is rendered on stderr, so it doesn't interfere.
    local output
    output=$(command try-rs "$@")

    if [ -n "$output" ]; then
        eval "$output"
    fi
}
"#;

    fs::write(&file_path, content)?;
    eprintln!("ZSH function file created at: {}", file_path.display());

    let home_dir = dirs::home_dir().expect("Could not find home directory");
    let zshrc_path = home_dir.join(".zshrc");
    let source_cmd = format!("source {}", file_path.display());

    if zshrc_path.exists() {
        let zshrc_content = fs::read_to_string(&zshrc_path)?;
        if !zshrc_content.contains(&source_cmd) {
            use std::io::Write;
            let mut file = fs::OpenOptions::new().append(true).open(&zshrc_path)?;
            writeln!(file, "\n# try-rs integration")?;
            writeln!(file, "{}", source_cmd)?;
            eprintln!("Added configuration to ~/.zshrc");
        } else {
            eprintln!("Configuration already present in ~/.zshrc");
        }
    } else {
        eprintln!("You need to source this file in your ~/.zshrc:");
        eprintln!("{}", source_cmd);
    }

    Ok(())
}

fn setup_bash() -> Result<()> {
    let config_dir = dirs::config_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .expect("Could not find home directory")
            .join(".config")
    });

    let app_config_dir = config_dir.join("try-rs");

    if !app_config_dir.exists() {
        fs::create_dir_all(&app_config_dir)?;
    }

    let file_path = app_config_dir.join("try-rs.bash");
    let content = r#"try-rs() {
    # Captures the output of the binary (stdout) which is the "cd" command
    # The TUI is rendered on stderr, so it doesn't interfere.
    local output
    output=$(command try-rs "$@")

    if [ -n "$output" ]; then
        eval "$output"
    fi
}
"#;

    fs::write(&file_path, content)?;
    eprintln!("Bash function file created at: {}", file_path.display());

    let home_dir = dirs::home_dir().expect("Could not find home directory");
    let bashrc_path = home_dir.join(".bashrc");
    let source_cmd = format!("source {}", file_path.display());

    if bashrc_path.exists() {
        let bashrc_content = fs::read_to_string(&bashrc_path)?;
        if !bashrc_content.contains(&source_cmd) {
            use std::io::Write;
            let mut file = fs::OpenOptions::new().append(true).open(&bashrc_path)?;
            writeln!(file, "\n# try-rs integration")?;
            writeln!(file, "{}", source_cmd)?;
            eprintln!("Added configuration to ~/.bashrc");
        } else {
            eprintln!("Configuration already present in ~/.bashrc");
        }
    } else {
        eprintln!("You need to source this file in your ~/.bashrc:");
        eprintln!("{}", source_cmd);
    }

    Ok(())
}

fn setup_powershell() -> Result<()> {
    // 1. Create the function file in the app's config dir
    let config_dir = dirs::config_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .expect("Could not find home directory")
            .join(".config")
    });
    let app_config_dir = config_dir.join("try-rs");
    if !app_config_dir.exists() {
        fs::create_dir_all(&app_config_dir)?;
    }

    let file_path = app_config_dir.join("try-rs.ps1");
    let content = r#"
# try-rs integration for PowerShell
function try-rs {
    # Captures the output of the binary (stdout) which is the "cd" or editor command
    # The TUI is rendered on stderr, so it doesn't interfere.
    $command = (try-rs.exe @args)

    if ($command) {
        Invoke-Expression $command
    }
}
"#;
    fs::write(&file_path, content.trim())?;
    eprintln!(
        "PowerShell function file created at: {}",
        file_path.display()
    );

    // 2. Find the PowerShell profile and add the source command
    let home_dir = dirs::home_dir().expect("Could not find home directory");
    let profile_path_ps7 = home_dir
        .join("Documents")
        .join("PowerShell")
        .join("Microsoft.PowerShell_profile.ps1");
    let profile_path_ps5 = home_dir
        .join("Documents")
        .join("WindowsPowerShell")
        .join("Microsoft.PowerShell_profile.ps1");

    let profile_path = if profile_path_ps7.exists() {
        profile_path_ps7
    } else if profile_path_ps5.exists() {
        profile_path_ps5
    } else {
        // If neither exists, default to the modern path.
        profile_path_ps7
    };

    if let Some(parent) = profile_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }

    // The command to add to the profile. Note the dot-sourcing.
    let source_cmd = format!(". '{}'", file_path.display());

    if profile_path.exists() {
        let profile_content = fs::read_to_string(&profile_path)?;
        if !profile_content.contains(&source_cmd) {
            use std::io::Write;
            let mut file = fs::OpenOptions::new().append(true).open(&profile_path)?;
            writeln!(file, "\n# try-rs integration")?;
            writeln!(file, "{}", source_cmd)?;
            eprintln!("Added configuration to {}", profile_path.display());
        } else {
            eprintln!(
                "Configuration already present in {}",
                profile_path.display()
            );
        }
    } else {
        let mut file = fs::File::create(&profile_path)?;
        writeln!(file, "# try-rs integration")?;
        writeln!(file, "{}", source_cmd)?;
        eprintln!(
            "PowerShell profile created and configured at: {}",
            profile_path.display()
        );
    }

    eprintln!(
        "You may need to restart your shell or run '. {}' to apply changes.",
        profile_path.display()
    );
    eprintln!(
        "If you get an error about running scripts, you may need to run: Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned"
    );

    Ok(())
}

fn setup_nushell() -> Result<()> {
    let config_dir = dirs::config_dir()
        .expect("Could not find config directory")
        .join("nushell");

    let app_config_dir = dirs::config_dir()
        .expect("Could not find config directory")
        .join("try-rs");

    if !app_config_dir.exists() {
        fs::create_dir_all(&app_config_dir)?;
    }

    let file_path = app_config_dir.join("try-rs.nu");
    let content = r#"def --wrapped try-rs [...args] {
    # Capture output. Stderr (TUI) goes directly to terminal.
    let output = (try-rs.exe ...$args)

    if ($output | is-not-empty) {

        # Grabs the path out of stdout returned by the binary and removes the single quotes
        let $path = ($output | split row ' ').1 | str replace --all "\'" ''
        cd $path
    }
}
"#;

    fs::write(&file_path, content)?;
    eprintln!("Nushell function created at: {}", file_path.display());

    // Modify config.nu to source the new file
    let nu_config_path = config_dir.join("config.nu");
    let source_cmd = format!("source {}", file_path.display());

    if nu_config_path.exists() {
        let nu_content = fs::read_to_string(&nu_config_path)?;
        if !nu_content.contains(&source_cmd) {
            use std::io::Write;
            let mut file = fs::OpenOptions::new().append(true).open(&nu_config_path)?;
            writeln!(file, "\n# try-rs integration")?;
            writeln!(file, "{}", source_cmd)?;
            eprintln!("Added configuration to {}", nu_config_path.display());
        } else {
            eprintln!(
                "Configuration already present in {}",
                nu_config_path.display()
            );
        }
    } else {
        eprintln!("Could not find config.nu at {}", nu_config_path.display());
        eprintln!("Please add the following line manually:");
        eprintln!("{}", source_cmd);
    }

    Ok(())
}

#[derive(Parser)]
#[command(name = "try-rs")]
#[command(about = format!("🦀 try-rs {} 🦀\nA blazing fast, Rust-based workspace manager for your temporary experiments.", env!("CARGO_PKG_VERSION")), long_about = None)]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    /// Create or jump to an experiment / Clone a repo. Starts the TUI (Terminal User Interface) if omitted.
    #[arg(value_name = "NAME_OR_URL")]
    name_or_url: Option<String>,

    /// Generate shell integration code
    #[arg(long)]
    setup: Option<Shell>,

    /// Shallow clone
    #[arg(short, long)]
    shallow_clone: bool,
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
enum Shell {
    Fish,
    Zsh,
    Bash,
    #[allow(clippy::enum_variant_names)]
    NuShell,
    #[allow(clippy::enum_variant_names)]
    PowerShell,
}

fn main() -> Result<()> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let mut stderr = std::io::stderr();
            write!(stderr, "{}", err).unwrap();
            std::process::exit(if err.use_stderr() { 1 } else { 0 });
        }
    };
    let (tries_dir, theme, editor_cmd, is_first_run) = load_configuration();

    // Ensure the directory exists (either from config or default)
    if !tries_dir.exists() {
        fs::create_dir_all(&tries_dir)?;
    }

    // Handle Shell Setup
    if let Some(shell) = cli.setup {
        match shell {
            Shell::Fish => setup_fish()?,
            Shell::Zsh => setup_zsh()?,
            Shell::Bash => setup_bash()?,
            Shell::PowerShell => setup_powershell()?,
            Shell::NuShell => setup_nushell()?,
        }
        return Ok(());
    }

    // Handle First Run / Interactive Setup
    if is_first_run && cli.setup.is_none() {
        let shell_type = if cfg!(windows) {
            // On Windows, PowerShell is the most likely modern shell.
            Some(Shell::PowerShell)
        } else {
            // Check for Nushell first
            if std::env::var("NU_VERSION").is_ok() {
                Some(Shell::NuShell)
            } else {
                let shell = std::env::var("SHELL").unwrap_or_default();
                if shell.contains("fish") {
                    Some(Shell::Fish)
                } else if shell.contains("zsh") {
                    Some(Shell::Zsh)
                } else if shell.contains("bash") {
                    Some(Shell::Bash)
                } else {
                    None
                }
            }
        };

        if let Some(s) = shell_type {
            eprintln!("Detected shell: {:?}", s);
            eprint!(
                "Shell integration not configured. Do you want to set it up for {:?}? [Y/n] ",
                s
            );
            io::stderr().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if input.trim().is_empty() || input.trim().eq_ignore_ascii_case("y") {
                match s {
                    Shell::Fish => setup_fish()?,
                    Shell::Zsh => setup_zsh()?,
                    Shell::Bash => setup_bash()?,
                    Shell::PowerShell => setup_powershell()?,
                    Shell::NuShell => setup_nushell()?,
                }
            }
        }
    }

    // The 'selection' variable will hold the chosen name or URL.
    // It can come from arguments (CLI) or the interface (TUI).
    let selection_result: Option<String>;
    let mut open_editor = false;

    if let Some(name) = cli.name_or_url {
        // CLI MODE: The user passed an argument (e.g., try-rs https://...)
        // We skip the graphical interface entirely.
        selection_result = Some(name);
    } else {
        // TUI MODE: No arguments, open the visual interface.

        enable_raw_mode()?;
        let mut stderr = io::stderr();
        execute!(stderr, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stderr);
        let mut terminal = Terminal::new(backend)?;

        let app = App::new(tries_dir.clone(), theme, editor_cmd.clone());
        // Run the app and capture the result
        let res = run_app(&mut terminal, app);

        // Restore the terminal
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        (selection_result, open_editor) = res?;
    }

    // 3. Process the result (Common for both modes)
    if let Some(selection) = selection_result {
        let target_path = tries_dir.join(&selection);

        // CASE 1: Does the folder already exist? Enter it.
        if target_path.exists() {
            if open_editor && let Some(cmd) = editor_cmd {
                println!("{} '{}'", cmd, target_path.to_string_lossy());
            } else {
                println!("cd '{}'", target_path.to_string_lossy());
            }
        } else {
            // CASE 2: Is it a Git URL? Clone it!
            if is_git_url(&selection) {
                let repo_name = extract_repo_name(&selection);

                let folder_name = repo_name;
                let new_path = tries_dir.join(&folder_name);

                eprintln!("Cloning {} into {}...", selection, folder_name);

                let mut cmd = std::process::Command::new("git");
                cmd.arg("clone");

                if cli.shallow_clone {
                    cmd.arg("--depth").arg("1");
                }

                let status = cmd
                    .arg(&selection)
                    .arg(&new_path)
                    .arg("--recurse-submodules")
                    .arg("--no-single-branch")
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .status();

                match status {
                    Ok(s) if s.success() => {
                        if open_editor && let Some(cmd) = editor_cmd {
                            println!("{} '{}'", cmd, new_path.to_string_lossy());
                        } else {
                            println!("cd '{}'", new_path.to_string_lossy());
                        }
                    }
                    _ => {
                        eprintln!("Error: Failed to clone the repository.");
                    }
                }
            } else {
                // CASE 3: Create an empty folder
                let new_name = selection;

                let new_path = tries_dir.join(&new_name);
                fs::create_dir_all(&new_path)?;
                if open_editor && let Some(cmd) = editor_cmd {
                    println!("{} '{}'", cmd, new_path.to_string_lossy());
                } else {
                    println!("cd '{}'", new_path.to_string_lossy());
                }
            }
        }
    }

    Ok(())
}

// Checks if the string looks like a Git URL
fn is_git_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("git@")
        || s.starts_with("ssh://")
        || s.ends_with(".git")
}

// Extracts a clean repository name (e.g., "github.com/tobi/try.git" -> "try")
fn extract_repo_name(url: &str) -> String {
    // Remove trailing slash and .git suffix
    let clean_url = url.trim_end_matches('/').trim_end_matches(".git");

    // Get the last part after the '/' or ':' (common in ssh)
    if let Some(last_part) = clean_url.rsplit(['/', ':']).next()
        && !last_part.is_empty()
    {
        return last_part.to_string();
    }
    // Generic name if detection fails
    "cloned-repo".to_string()
}
