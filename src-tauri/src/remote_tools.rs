//! Shared helpers for agent-facing remote file tools.

/// Options for a remote text search.
#[derive(Debug, Clone)]
pub struct RemoteSearchOptions {
    pub pattern: String,
    pub path: String,
    pub ignore_case: bool,
    pub fixed_strings: bool,
    pub hidden: bool,
    pub globs: Vec<String>,
    pub max_results: usize,
}

impl RemoteSearchOptions {
    pub fn new(pattern: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            path: path.into(),
            ignore_case: false,
            fixed_strings: false,
            hidden: false,
            globs: Vec::new(),
            max_results: 200,
        }
    }
}

/// Quote a string for POSIX shell usage.
///
/// Canonical implementation lives in the `vibeshell-plugins` crate so plugin
/// command rendering and the rest of the app share one quoting boundary.
pub use vibeshell_plugins::shell_quote;

/// Build a remote command that searches with ripgrep and falls back to grep.
pub fn build_remote_rg_command(options: &RemoteSearchOptions) -> String {
    let path = if options.path.trim().is_empty() {
        "."
    } else {
        options.path.as_str()
    };
    let max_results = options.max_results.max(1);

    let mut rg_args = vec![
        "rg".to_string(),
        "--line-number".to_string(),
        "--column".to_string(),
        "--color".to_string(),
        "never".to_string(),
        "--heading".to_string(),
        "never".to_string(),
        "--no-messages".to_string(),
    ];

    if options.ignore_case {
        rg_args.push("--ignore-case".to_string());
    }
    if options.fixed_strings {
        rg_args.push("--fixed-strings".to_string());
    }
    if options.hidden {
        rg_args.push("--hidden".to_string());
    }
    for glob in &options.globs {
        rg_args.push("--glob".to_string());
        rg_args.push(glob.clone());
    }
    rg_args.push("--".to_string());
    rg_args.push(options.pattern.clone());
    rg_args.push(path.to_string());

    let mut grep_args = vec![
        "grep".to_string(),
        "-RIn".to_string(),
        "--binary-files=without-match".to_string(),
        "--exclude-dir=.git".to_string(),
    ];
    if options.ignore_case {
        grep_args.push("-i".to_string());
    }
    if options.fixed_strings {
        grep_args.push("-F".to_string());
    }
    grep_args.push("--".to_string());
    grep_args.push(options.pattern.clone());
    grep_args.push(path.to_string());

    let script = format!(
        "if command -v rg >/dev/null 2>&1; then {}; else {}; fi | head -n {}",
        join_shell_args(&rg_args),
        join_shell_args(&grep_args),
        max_results
    );

    format!("sh -lc {}", shell_quote(&script))
}

fn join_shell_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_handles_spaces_and_quotes() {
        assert_eq!(shell_quote("/srv/app"), "/srv/app");
        assert_eq!(shell_quote("hello world"), "'hello world'");
        assert_eq!(shell_quote("it's"), "'it'\"'\"'s'");
    }

    #[test]
    fn build_remote_rg_command_quotes_pattern_and_path() {
        let mut options = RemoteSearchOptions::new("foo bar", "/srv/my app");
        options.ignore_case = true;
        options.globs.push("*.rs".to_string());
        options.max_results = 50;

        let command = build_remote_rg_command(&options);

        assert!(command.starts_with("sh -lc "));
        assert!(command.contains("'foo bar'"));
        assert!(command.contains("'/srv/my app'"));
        assert!(command.contains("--ignore-case"));
        assert!(command.contains("--glob"));
        assert!(command.contains("*.rs"));
        assert!(command.contains("head -n 50"));
    }
}
