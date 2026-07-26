use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;
const MAX_GIT_STDERR_BYTES: usize = 64 * 1024;

struct BoundedGitOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    truncated: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatusRequest {
    pub cwd: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDiffRequest {
    pub cwd: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitFileKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitWorkspaceFile {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: GitFileKind,
    pub staged: bool,
    pub unstaged: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorkspaceStatus {
    pub root: String,
    pub branch: Option<String>,
    pub files: Vec<GitWorkspaceFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorkspaceDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub content: String,
    pub truncated: bool,
}

pub fn get_workspace_status(request: WorkspaceStatusRequest) -> Result<GitWorkspaceStatus> {
    workspace_status(Path::new(&request.cwd))
}

pub fn get_workspace_diff(request: WorkspaceDiffRequest) -> Result<GitWorkspaceDiff> {
    validate_relative_path(&request.path)?;
    let status = workspace_status(Path::new(&request.cwd))?;
    let file = status
        .files
        .iter()
        .find(|file| file.path == request.path)
        .ok_or_else(|| anyhow!("File is no longer present in the working tree changes"))?;
    let root = PathBuf::from(&status.root);

    let output = if file.kind == GitFileKind::Untracked {
        untracked_diff(&root, &file.path)?
    } else {
        tracked_diff(&root, file)?
    };
    let (content, truncated) = bounded_lossy_utf8(&output.stdout, output.truncated);

    Ok(GitWorkspaceDiff {
        path: file.path.clone(),
        old_path: file.old_path.clone(),
        content,
        truncated,
    })
}

fn workspace_status(cwd: &Path) -> Result<GitWorkspaceStatus> {
    let cwd = cwd
        .canonicalize()
        .with_context(|| format!("Workspace does not exist: {}", cwd.display()))?;
    if !cwd.is_dir() {
        bail!("Workspace is not a directory: {}", cwd.display());
    }

    let root_output = run_git(&cwd, &["rev-parse", "--show-toplevel"])?;
    ensure_success(&root_output, "Workspace is not a Git repository")?;
    let root = PathBuf::from(String::from_utf8_lossy(&root_output.stdout).trim());

    let branch_output = run_git(&root, &["branch", "--show-current"])?;
    let mut branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();
    if branch.is_empty() {
        if let Ok(output) = run_git(&root, &["rev-parse", "--short", "HEAD"]) {
            if output.status.success() {
                branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
        }
    }

    let status_output = run_git(
        &root,
        &[
            "-c",
            "core.quotepath=false",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
        ],
    )?;
    ensure_success(&status_output, "Could not read Git working tree status")?;

    Ok(GitWorkspaceStatus {
        root: root.to_string_lossy().into_owned(),
        branch: (!branch.is_empty()).then_some(branch),
        files: parse_porcelain_v1_z(&status_output.stdout),
    })
}

fn tracked_diff(root: &Path, file: &GitWorkspaceFile) -> Result<BoundedGitOutput> {
    let pathspec = literal_pathspec(&file.path);
    let old_pathspec = file.old_path.as_deref().map(literal_pathspec);
    let mut args = vec![
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--find-renames",
        "--unified=3",
        "HEAD",
        "--",
    ];
    if let Some(old_pathspec) = old_pathspec.as_deref() {
        args.push(old_pathspec);
    }
    args.push(&pathspec);

    let output = run_git_bounded(root, &args)?;
    if output.status.success() {
        return Ok(output);
    }

    // Repositories without an initial commit do not have HEAD yet.
    let cached = run_git_bounded(
        root,
        &[
            "diff",
            "--cached",
            "--no-color",
            "--no-ext-diff",
            "--unified=3",
            "--",
            &pathspec,
        ],
    )?;
    let unstaged = run_git_bounded(
        root,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--unified=3",
            "--",
            &pathspec,
        ],
    )?;

    ensure_bounded_success(&cached, "Could not read staged changes")?;
    ensure_bounded_success(&unstaged, "Could not read unstaged changes")?;

    let mut stdout =
        Vec::with_capacity(MAX_DIFF_BYTES.min(cached.stdout.len() + unstaged.stdout.len()));
    let mut truncated = cached.truncated || unstaged.truncated;
    append_bounded(&mut stdout, &cached.stdout, &mut truncated);
    if !stdout.is_empty() && !unstaged.stdout.is_empty() {
        append_bounded(&mut stdout, b"\n", &mut truncated);
    }
    append_bounded(&mut stdout, &unstaged.stdout, &mut truncated);
    Ok(BoundedGitOutput {
        status: cached.status,
        stdout,
        stderr: [cached.stderr, unstaged.stderr].concat(),
        truncated,
    })
}

fn literal_pathspec(path: &str) -> String {
    format!(":(literal){path}")
}

fn untracked_diff(root: &Path, relative_path: &str) -> Result<BoundedGitOutput> {
    let full_path = root.join(relative_path);
    if !full_path.is_file() {
        bail!("Untracked path is not a regular file");
    }

    #[cfg(target_os = "windows")]
    let null_device = "NUL";
    #[cfg(not(target_os = "windows"))]
    let null_device = "/dev/null";

    let output = run_git_bounded(
        root,
        &[
            "diff",
            "--no-index",
            "--no-color",
            "--unified=3",
            "--",
            null_device,
            relative_path,
        ],
    )?;
    // git diff --no-index uses exit code 1 when differences are present.
    if output.status.success() || output.status.code() == Some(1) {
        Ok(output)
    } else {
        ensure_bounded_success(&output, "Could not build diff for untracked file")?;
        unreachable!()
    }
}

fn run_git_bounded(cwd: &Path, args: &[&str]) -> Result<BoundedGitOutput> {
    let mut child = Command::new("git")
        .current_dir(cwd)
        .env("LC_ALL", "C")
        .env("GIT_PAGER", "cat")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Git executable was not found")?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Could not capture Git stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Could not capture Git stderr"))?;

    let stdout_reader = thread::spawn(move || read_bounded(stdout, MAX_DIFF_BYTES));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, MAX_GIT_STDERR_BYTES));
    let status = child.wait().context("Could not wait for Git")?;
    let (stdout, truncated) = stdout_reader
        .join()
        .map_err(|_| anyhow!("Git stdout reader thread panicked"))??;
    let (stderr, _) = stderr_reader
        .join()
        .map_err(|_| anyhow!("Git stderr reader thread panicked"))??;

    Ok(BoundedGitOutput {
        status,
        stdout,
        stderr,
        truncated,
    })
}

fn read_bounded(mut reader: impl Read, limit: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::with_capacity(limit.min(64 * 1024));
    let mut truncated = false;
    let mut buffer = [0u8; 8192];

    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }

        let remaining = limit.saturating_sub(output.len());
        let kept = remaining.min(count);
        output.extend_from_slice(&buffer[..kept]);
        if kept < count {
            truncated = true;
        }
    }

    Ok((output, truncated))
}

fn append_bounded(output: &mut Vec<u8>, value: &[u8], truncated: &mut bool) {
    let remaining = MAX_DIFF_BYTES.saturating_sub(output.len());
    let kept = remaining.min(value.len());
    output.extend_from_slice(&value[..kept]);
    *truncated |= kept < value.len();
}

fn bounded_lossy_utf8(bytes: &[u8], already_truncated: bool) -> (String, bool) {
    let mut content = String::from_utf8_lossy(bytes).into_owned();
    let decoded_truncated = content.len() > MAX_DIFF_BYTES;
    if decoded_truncated {
        let mut boundary = MAX_DIFF_BYTES;
        while !content.is_char_boundary(boundary) {
            boundary -= 1;
        }
        content.truncate(boundary);
    }
    (content, already_truncated || decoded_truncated)
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .current_dir(cwd)
        .env("LC_ALL", "C")
        .args(args)
        .output()
        .context("Git executable was not found")
}

fn ensure_success(output: &Output, context: &str) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        bail!("{context}");
    }
    bail!("{context}: {stderr}")
}

fn ensure_bounded_success(output: &BoundedGitOutput, context: &str) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        bail!("{context}");
    }
    bail!("{context}: {stderr}")
}

fn validate_relative_path(path: &str) -> Result<()> {
    if path.is_empty() || path.contains('\0') {
        bail!("Invalid workspace path");
    }
    if Path::new(path).components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        bail!("Workspace path must stay inside the repository");
    }
    Ok(())
}

fn parse_porcelain_v1_z(bytes: &[u8]) -> Vec<GitWorkspaceFile> {
    let mut records = bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    let mut files = Vec::new();

    while let Some(record) = records.next() {
        if record.len() < 4 || record[2] != b' ' {
            continue;
        }
        let index = record[0] as char;
        let worktree = record[1] as char;
        let path = String::from_utf8_lossy(&record[3..]).into_owned();
        let old_path = if matches!(index, 'R' | 'C') || matches!(worktree, 'R' | 'C') {
            records
                .next()
                .map(|value| String::from_utf8_lossy(value).into_owned())
        } else {
            None
        };

        files.push(GitWorkspaceFile {
            kind: file_kind(index, worktree),
            staged: index != ' ' && index != '?',
            unstaged: worktree != ' ' || (index == '?' && worktree == '?'),
            path,
            old_path,
        });
    }

    files
}

fn file_kind(index: char, worktree: char) -> GitFileKind {
    let pair = [index, worktree];
    if pair == ['?', '?'] {
        GitFileKind::Untracked
    } else if pair.contains(&'U') || matches!((index, worktree), ('A', 'A') | ('D', 'D')) {
        GitFileKind::Conflicted
    } else if pair.iter().any(|status| matches!(*status, 'R' | 'C')) {
        GitFileKind::Renamed
    } else if pair.contains(&'D') {
        GitFileKind::Deleted
    } else if pair.contains(&'A') {
        GitFileKind::Added
    } else {
        GitFileKind::Modified
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;

    #[test]
    fn bounded_reader_drains_input_without_retaining_the_tail() {
        let (content, truncated) = read_bounded(Cursor::new(b"abcdefgh"), 4).unwrap();
        assert_eq!(content, b"abcd");
        assert!(truncated);
    }

    #[test]
    fn truncates_invalid_utf8_expansion_at_a_character_boundary() {
        let bytes = vec![0xff; MAX_DIFF_BYTES];
        let (content, truncated) = bounded_lossy_utf8(&bytes, false);

        assert!(truncated);
        assert!(content.len() <= MAX_DIFF_BYTES);
        assert!(MAX_DIFF_BYTES - content.len() < '\u{fffd}'.len_utf8());
        assert_eq!(content.len() % '\u{fffd}'.len_utf8(), 0);

        let (_, truncated) = bounded_lossy_utf8(b"complete", true);
        assert!(truncated);
    }

    #[test]
    fn parses_porcelain_records_and_rename_pairs() {
        let files = parse_porcelain_v1_z(
            b" M src/main.rs\0A  new file.txt\0R  renamed.rs\0old.rs\0?? notes.md\0UU conflict.ts\0",
        );

        assert_eq!(files.len(), 5);
        assert_eq!(files[0].kind, GitFileKind::Modified);
        assert!(files[0].unstaged);
        assert_eq!(files[1].kind, GitFileKind::Added);
        assert!(files[1].staged);
        assert_eq!(files[2].kind, GitFileKind::Renamed);
        assert_eq!(files[2].old_path.as_deref(), Some("old.rs"));
        assert_eq!(files[3].kind, GitFileKind::Untracked);
        assert_eq!(files[4].kind, GitFileKind::Conflicted);
    }

    #[test]
    fn rejects_paths_that_escape_the_repository() {
        assert!(validate_relative_path("src/main.rs").is_ok());
        assert!(validate_relative_path("../secrets").is_err());
        assert!(validate_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn reads_real_git_status_and_diff() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let run = |args: &[&str]| {
            let output = Command::new("git")
                .current_dir(root)
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {:?} failed", args);
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "tests@vibeshell.local"]);
        run(&["config", "user.name", "VibeShell Tests"]);
        fs::write(root.join("tracked.txt"), "before\n").unwrap();
        run(&["add", "tracked.txt"]);
        run(&["commit", "-qm", "initial"]);
        fs::write(root.join("tracked.txt"), "after\n").unwrap();
        fs::write(root.join("new.txt"), "new\n").unwrap();

        let status = workspace_status(root).unwrap();
        assert_eq!(status.files.len(), 2);

        let diff = get_workspace_diff(WorkspaceDiffRequest {
            cwd: root.to_string_lossy().into_owned(),
            path: "tracked.txt".into(),
        })
        .unwrap();
        assert!(diff.content.contains("-before"));
        assert!(diff.content.contains("+after"));
    }

    #[test]
    fn tracked_diff_treats_pathspec_magic_as_a_literal_filename() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let run = |args: &[&str]| {
            let output = Command::new("git")
                .current_dir(root)
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {:?} failed", args);
        };
        // Brackets exercise Git's pathspec magic while remaining a valid filename on Windows.
        let filename = "[tracked].txt";

        run(&["init", "-q"]);
        run(&["config", "user.email", "tests@vibeshell.local"]);
        run(&["config", "user.name", "VibeShell Tests"]);
        fs::write(root.join(filename), "before\n").unwrap();
        run(&["add", "--all"]);

        let staged_diff = get_workspace_diff(WorkspaceDiffRequest {
            cwd: root.to_string_lossy().into_owned(),
            path: filename.into(),
        })
        .unwrap();
        assert!(staged_diff.content.contains("+before"));

        run(&["commit", "-qm", "initial"]);
        fs::write(root.join(filename), "after\n").unwrap();

        let worktree_diff = get_workspace_diff(WorkspaceDiffRequest {
            cwd: root.to_string_lossy().into_owned(),
            path: filename.into(),
        })
        .unwrap();
        assert!(worktree_diff.content.contains("-before"));
        assert!(worktree_diff.content.contains("+after"));
    }

    #[test]
    fn truncates_large_untracked_diff_while_capturing() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let output = Command::new("git")
            .current_dir(root)
            .args(["init", "-q"])
            .output()
            .unwrap();
        assert!(output.status.success());
        fs::write(root.join("large.txt"), vec![b'x'; MAX_DIFF_BYTES + 4096]).unwrap();

        let diff = get_workspace_diff(WorkspaceDiffRequest {
            cwd: root.to_string_lossy().into_owned(),
            path: "large.txt".into(),
        })
        .unwrap();

        assert!(diff.truncated);
        assert!(diff.content.len() <= MAX_DIFF_BYTES);
    }
}
