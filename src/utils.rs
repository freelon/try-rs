use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use chrono::{Local, NaiveDate, NaiveDateTime};

const DATE_PREFIX_FORMAT: &str = "%Y-%m-%d";

/// Checks if current directory is inside a git repository
pub fn is_inside_git_repo<P: AsRef<Path>>(path: P) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path.as_ref())
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn is_git_worktree_locked(path: &Path) -> bool {
    let dot_git = path.join(".git");
    if dot_git.is_file() {
        let parent = parse_dot_git(&dot_git);
        match parent {
            Ok(parent_path) => {
                return parent_path.join("locked").exists();
            }
            Err(_) => {
                return false;
            }
        }
    }
    false
}

/// Checks if a path is a git worktree (not the main working tree)
/// A worktree has a .git file (not directory) that points to the main repo
pub fn is_git_worktree(path: &Path) -> bool {
    let dot_git = path.join(".git");
    // If .git is a file (not a directory), it's a worktree
    dot_git.is_file()
}

fn parse_dot_git(dot_git: &Path) -> std::io::Result<PathBuf> {
    Ok(first_line(&std::fs::read(dot_git)?).into())
}

#[cfg(unix)]
pub fn first_line(bytes: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(
        bytes
            .iter()
            .copied()
            .skip_while(|&b| b != b' ')
            .skip(1)
            .take_while(|&b| b != b'\n')
            .collect::<Vec<_>>(),
    )
}

#[cfg(not(unix))]
pub fn first_line(bytes: &[u8]) -> OsString {
    let vec: Vec<u8> = bytes
        .iter()
        .copied()
        .skip_while(|&b| b != b' ')
        .skip(1)
        .take_while(|&b| b != b'\n')
        .collect();
    OsString::from(String::from_utf8_lossy(&vec).to_string())
}

pub fn remove_git_worktree(path_to_remove: &Path) -> std::io::Result<std::process::Output> {
    Command::new("git")
        .args(["worktree", "remove", "."])
        .current_dir(path_to_remove)
        .output()
}

pub fn expand_path(path_str: &str) -> PathBuf {
    if (path_str.starts_with("~/") || (cfg!(windows) && path_str.starts_with("~\\")))
        && let Some(home) = dirs::home_dir()
    {
        return home.join(&path_str[2..]);
    }
    PathBuf::from(path_str)
}

pub fn is_git_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("git@")
        || s.starts_with("ssh://")
        || s.ends_with(".git")
}

pub fn extract_repo_name(url: &str) -> String {
    let clean_url = url.trim_end_matches('/').trim_end_matches(".git");
    if let Some(last_part) = clean_url.rsplit(['/', ':']).next()
        && !last_part.is_empty()
    {
        return last_part.to_string();
    }
    "cloned-repo".to_string()
}

#[cfg(unix)]
pub fn get_free_disk_space_mb(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();

    unsafe {
        if libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) == 0 {
            let stat = stat.assume_init();
            let free_bytes = (stat.f_bavail as u64) * (stat.f_frsize as u64);
            return Some(free_bytes / (1024 * 1024));
        }
    }
    None
}

#[cfg(not(unix))]
pub fn get_free_disk_space_mb(_path: &Path) -> Option<u64> {
    None
}

pub fn extract_prefix_date(name: &str) -> Option<(SystemTime, String)> {
    let (lhs, rhs) = name.split_once(' ')?;
    let naive_date = NaiveDate::parse_from_str(lhs, DATE_PREFIX_FORMAT).ok()?;
    let dt: NaiveDateTime = naive_date.into();
    let dt_local = dt.and_local_timezone(Local).single()?;
    Some((dt_local.into(), rhs.into()))
}

pub fn generate_prefix_date() -> String {
    let now = Local::now();
    now.format("%Y-%m-%d").to_string()
}

pub fn get_folder_size_mb(path: &Path) -> u64 {
    fn dir_size(path: &Path) -> u64 {
        let mut stack = vec![path.to_path_buf()];
        let mut size = 0u64;
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                // Use symlink_metadata to avoid following symlinks
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                if meta.is_dir() {
                    stack.push(entry.path());
                } else if meta.is_file() {
                    size += meta.len();
                }
                // Symlinks and other special files are intentionally skipped
            }
        }
        size
    }
    dir_size(path) / (1024 * 1024)
}

pub fn matching_folders(name: &str, path: &PathBuf) -> Vec<String> {
    let mut result = vec![];
    if let Ok(read_dir) = fs::read_dir(&path) {
        for entry in read_dir.flatten() {
            if let Ok(metadata) = entry.metadata()
                && metadata.is_dir()
            {
                let filename = entry.file_name().to_string_lossy().to_string();
                if filename == name {
                    result.push(filename);
                } else if let Some((_, stripped_name)) = extract_prefix_date(&filename)
                    && name == stripped_name
                {
                    result.push(filename);
                }
            }
        }
    }
    result
}

// i've put this here since until now there is not really a library part
pub enum SelectionResult {
    /// A explicit folder that is guaranteed to exist already
    Folder(String),
    /// No existing match, a new folder should be created
    New(String),
    /// Nothing was selected in the UI, quit
    None,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempdir::TempDir;

    #[test]
    fn is_git_url_valid_urls() {
        assert!(is_git_url("https://github.com/user/repo.git"));
        assert!(is_git_url("http://github.com/user/repo"));
        assert!(is_git_url("git@github.com:user/repo.git"));
        assert!(is_git_url("ssh://git@github.com/user/repo"));
        assert!(is_git_url("some-repo.git"));
    }

    #[test]
    fn is_git_url_rejects_plain_names() {
        assert!(!is_git_url("my-project"));
        assert!(!is_git_url("foo/bar"));
        assert!(!is_git_url(""));
    }

    #[test]
    fn extract_repo_name_from_https() {
        assert_eq!(
            extract_repo_name("https://github.com/user/repo.git"),
            "repo"
        );
        assert_eq!(
            extract_repo_name("https://github.com/user/repo"),
            "repo"
        );
    }

    #[test]
    fn extract_repo_name_from_ssh() {
        assert_eq!(extract_repo_name("git@github.com:user/repo.git"), "repo");
    }

    #[test]
    fn extract_repo_name_trailing_slash() {
        assert_eq!(
            extract_repo_name("https://github.com/user/repo/"),
            "repo"
        );
    }

    #[test]
    fn extract_prefix_date_valid() {
        let result = extract_prefix_date("2024-06-15 my-project");
        assert!(result.is_some());
        let (_, name) = result.unwrap();
        assert_eq!(name, "my-project");
    }

    #[test]
    fn extract_prefix_date_invalid() {
        assert!(extract_prefix_date("not-a-date project").is_none());
        assert!(extract_prefix_date("nodate").is_none());
    }

    #[test]
    fn generate_prefix_date_format() {
        let date = generate_prefix_date();
        // Format is YYYY-MM-DD
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
    }

    #[test]
    fn expand_path_tilde() {
        let expanded = expand_path("~/some/dir");
        assert!(!expanded.starts_with("~"));
        assert!(expanded.to_string_lossy().ends_with("some/dir"));
    }

    #[test]
    fn expand_path_absolute() {
        let expanded = expand_path("/absolute/path");
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn matching_folders_exact_and_dated() {
        let tmp = TempDir::new("match-test").unwrap();
        let base = tmp.path();
        fs::create_dir(base.join("foo")).unwrap();
        fs::create_dir(base.join("2024-01-15 foo")).unwrap();
        fs::create_dir(base.join("bar")).unwrap();

        let matches = matching_folders("foo", &base.to_path_buf());
        assert!(matches.contains(&"foo".to_string()));
        assert!(matches.contains(&"2024-01-15 foo".to_string()));
        assert!(!matches.contains(&"bar".to_string()));
    }

    #[test]
    fn get_folder_size_mb_empty() {
        let tmp = TempDir::new("size-test").unwrap();
        assert_eq!(get_folder_size_mb(tmp.path()), 0);
    }

    #[test]
    fn get_folder_size_mb_nonexistent() {
        assert_eq!(get_folder_size_mb(Path::new("/nonexistent/path")), 0);
    }
}
