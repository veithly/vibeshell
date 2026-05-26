use std::fs;
use std::io::Read;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;

#[derive(Args, Clone, Debug, Default)]
pub struct CommandInputArgs {
    /// Read the full remote command from a local text file
    #[arg(long = "command-file", value_name = "PATH")]
    pub command_file: Option<PathBuf>,

    /// Read the full remote command from stdin instead of shell-parsed arguments
    #[arg(long = "command-stdin")]
    pub command_stdin: bool,

    /// Run a single remote command instead of opening an interactive shell
    #[arg(last = true)]
    pub command: Vec<String>,
}

impl CommandInputArgs {
    pub fn resolve(&self) -> Result<Vec<String>> {
        let mut sources = 0;
        if self.command_file.is_some() {
            sources += 1;
        }
        if self.command_stdin {
            sources += 1;
        }
        if !self.command.is_empty() {
            sources += 1;
        }

        if sources > 1 {
            bail!(
                "Use only one command source: `-- <command>`, `--command-file <path>`, or `--command-stdin`."
            );
        }

        if let Some(path) = &self.command_file {
            let raw = fs::read_to_string(path).with_context(|| {
                format!(
                    "Failed to read remote command from file '{}'",
                    path.display()
                )
            })?;
            return command_from_loaded_text(raw, &format!("file '{}'", path.display()));
        }

        if self.command_stdin {
            let mut raw = String::new();
            std::io::stdin()
                .read_to_string(&mut raw)
                .context("Failed to read remote command from stdin")?;
            return command_from_loaded_text(raw, "stdin");
        }

        Ok(self.command.clone())
    }
}

fn command_from_loaded_text(raw: String, source: &str) -> Result<Vec<String>> {
    let normalized = normalize_command_text(&raw);
    if normalized.is_empty() {
        bail!("No remote command text was provided via {}.", source);
    }
    Ok(vec![normalized])
}

fn normalize_command_text(raw: &str) -> String {
    raw.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_end_matches('\n')
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{normalize_command_text, CommandInputArgs};

    #[test]
    fn keeps_inline_command_tokens_unchanged() {
        let args = CommandInputArgs {
            command: vec!["uname".to_string(), "-a".to_string()],
            ..Default::default()
        };

        let resolved = args.resolve().expect("inline command should resolve");
        assert_eq!(resolved, vec!["uname".to_string(), "-a".to_string()]);
    }

    #[test]
    fn normalizes_loaded_text_line_endings() {
        let normalized = normalize_command_text("echo one\r\necho two\r\n");
        assert_eq!(normalized, "echo one\necho two");
    }

    #[test]
    fn rejects_multiple_command_sources() {
        let args = CommandInputArgs {
            command_file: Some(PathBuf::from("cmd.sh")),
            command: vec!["hostname".to_string()],
            ..Default::default()
        };

        let error = args.resolve().expect_err("multiple sources must fail");
        assert!(
            error.to_string().contains("Use only one command source"),
            "unexpected error: {}",
            error
        );
    }

    #[test]
    fn reads_command_from_file() {
        let mut path = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        path.push(format!("vshell-command-{unique}.txt"));

        std::fs::write(&path, "echo one\r\necho two\r\n").expect("temp file should be written");

        let args = CommandInputArgs {
            command_file: Some(path.clone()),
            ..Default::default()
        };

        let resolved = args.resolve().expect("file-backed command should resolve");
        assert_eq!(resolved, vec!["echo one\necho two".to_string()]);

        let _ = std::fs::remove_file(path);
    }
}
