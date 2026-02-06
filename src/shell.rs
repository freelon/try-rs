use crate::cli::Shell;
use crate::config::{get_base_config_dir, get_config_dir};
use anyhow::Result;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Returns the shell integration script content for the given shell type.
/// This is used by --setup-stdout to print the content to stdout.
pub fn get_shell_content(shell: &Shell) -> &'static str {
    match shell {
        Shell::Fish => {
            r#"function try-rs
    # Pass flags/options directly to stdout without capturing
    for arg in $argv
        if string match -q -- '-*' $arg
            command try-rs $argv
            return
        end
    end

    # Captures the output of the binary (stdout) which is the "cd" command
    # The TUI is rendered on stderr, so it doesn't interfere.
    set command (command try-rs $argv | string collect)

    if test -n "$command"
        eval $command
    end
end
"#
        }
        Shell::Zsh => {
            r#"try-rs() {
    # Pass flags/options directly to stdout without capturing
    for arg in "$@"; do
        case "$arg" in
            -*) command try-rs "$@"; return ;;
        esac
    done

    # Captures the output of the binary (stdout) which is the "cd" command
    # The TUI is rendered on stderr, so it doesn't interfere.
    local output
    output=$(command try-rs "$@")

    if [ -n "$output" ]; then
        eval "$output"
    fi
}
"#
        }
        Shell::Bash => {
            r#"try-rs() {
    # Pass flags/options directly to stdout without capturing
    for arg in "$@"; do
        case "$arg" in
            -*) command try-rs "$@"; return ;;
        esac
    done

    # Captures the output of the binary (stdout) which is the "cd" command
    # The TUI is rendered on stderr, so it doesn't interfere.
    local output
    output=$(command try-rs "$@")

    if [ -n "$output" ]; then
        eval "$output"
    fi
}
"#
        }
        Shell::PowerShell => {
            r#"# try-rs integration for PowerShell
function try-rs {
    # Pass flags/options directly to stdout without capturing
    foreach ($a in $args) {
        if ($a -like '-*') {
            & try-rs.exe @args
            return
        }
    }

    # Captures the output of the binary (stdout) which is the "cd" or editor command
    # The TUI is rendered on stderr, so it doesn't interfere.
    $command = (try-rs.exe @args)

    if ($command) {
        Invoke-Expression $command
    }
}
"#
        }
        Shell::NuShell => {
            r#"def --wrapped try-rs [...args] {
    # Pass flags/options directly to stdout without capturing
    for arg in $args {
        if ($arg | str starts-with '-') {
            ^try-rs.exe ...$args
            return
        }
    }

    # Capture output. Stderr (TUI) goes directly to terminal.
    let output = (try-rs.exe ...$args)

    if ($output | is-not-empty) {

        # Grabs the path out of stdout returned by the binary and removes the single quotes
        let $path = ($output | split row ' ').1 | str replace --all "'" ''
        cd $path
    }
}
"#
        }
    }
}

pub fn get_shell_integration_path(shell: &Shell) -> PathBuf {
    let config_dir = match shell {
        Shell::Fish => get_base_config_dir(),
        _ => get_config_dir(),
    };

    match shell {
        Shell::Fish => config_dir
            .join("fish")
            .join("functions")
            .join("try-rs.fish"),
        Shell::Zsh => config_dir.join("try-rs.zsh"),
        Shell::Bash => config_dir.join("try-rs.bash"),
        Shell::PowerShell => config_dir.join("try-rs.ps1"),
        Shell::NuShell => config_dir.join("try-rs.nu"),
    }
}

pub fn is_shell_integration_configured(shell: &Shell) -> bool {
    get_shell_integration_path(shell).exists()
}

/// Appends a source command to an RC file if not already present.
fn append_source_to_rc(rc_path: &std::path::Path, source_cmd: &str) -> Result<()> {
    if rc_path.exists() {
        let content = fs::read_to_string(rc_path)?;
        if !content.contains(source_cmd) {
            let mut file = fs::OpenOptions::new().append(true).open(rc_path)?;
            writeln!(file, "\n# try-rs integration")?;
            writeln!(file, "{}", source_cmd)?;
            eprintln!("Added configuration to {}", rc_path.display());
        } else {
            eprintln!("Configuration already present in {}", rc_path.display());
        }
    } else {
        eprintln!("You need to add the following line to {}:", rc_path.display());
        eprintln!("{}", source_cmd);
    }
    Ok(())
}

/// Writes the shell integration file and returns its path.
fn write_shell_integration(shell: &Shell) -> Result<std::path::PathBuf> {
    let file_path = get_shell_integration_path(shell);
    if let Some(parent) = file_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(&file_path, get_shell_content(shell))?;
    eprintln!("{:?} function file created at: {}", shell, file_path.display());
    Ok(file_path)
}

/// Sets up shell integration for the given shell.
pub fn setup_shell(shell: &Shell) -> Result<()> {
    let file_path = write_shell_integration(shell)?;
    let home_dir = dirs::home_dir().expect("Could not find home directory");

    match shell {
        Shell::Fish => {
            eprintln!(
                "You may need to restart your shell or run 'source {}' to apply changes.",
                file_path.display()
            );
        }
        Shell::Zsh => {
            let source_cmd = format!("source '{}'", file_path.display());
            append_source_to_rc(&home_dir.join(".zshrc"), &source_cmd)?;
        }
        Shell::Bash => {
            let source_cmd = format!("source '{}'", file_path.display());
            append_source_to_rc(&home_dir.join(".bashrc"), &source_cmd)?;
        }
        Shell::PowerShell => {
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
                profile_path_ps7
            };

            if let Some(parent) = profile_path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent)?;
            }

            let source_cmd = format!(". '{}'", file_path.display());
            if profile_path.exists() {
                append_source_to_rc(&profile_path, &source_cmd)?;
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
        }
        Shell::NuShell => {
            let nu_config_path = dirs::config_dir()
                .expect("Could not find config directory")
                .join("nushell")
                .join("config.nu");
            let source_cmd = format!("source '{}'", file_path.display());
            if nu_config_path.exists() {
                append_source_to_rc(&nu_config_path, &source_cmd)?;
            } else {
                eprintln!("Could not find config.nu at {}", nu_config_path.display());
                eprintln!("Please add the following line manually:");
                eprintln!("{}", source_cmd);
            }
        }
    }

    Ok(())
}
