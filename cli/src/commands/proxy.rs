use anyhow::{bail, Context, Result};
use clap::{Subcommand, ValueEnum};
use vibeshell_core::network::{ProxyMode, ProxySettings, ProxySettingsInput};

#[derive(Clone, Copy, ValueEnum)]
pub enum Mode {
    Direct,
    System,
    Custom,
}
#[derive(Subcommand)]
pub enum ProxyCommand {
    /// Show routing configuration, never the password.
    Show,
    /// Persist an outbound route shared by the desktop and daemon.
    Set {
        #[arg(long, value_enum)]
        mode: Mode,
        #[arg(long, default_value = "")]
        url: String,
        #[arg(long, default_value = "")]
        username: String,
        /// Read the exact password bytes from a protected pipe, never argv.
        #[arg(long, conflicts_with = "clear_password")]
        password_stdin: bool,
        #[arg(long)]
        clear_password: bool,
    },
}
pub fn run(command: ProxyCommand) -> Result<()> {
    use std::io::{IsTerminal, Read};
    let db = vibeshell_core::Database::new()?;
    let settings = match command {
        ProxyCommand::Show => ProxySettings::load(&db)?,
        ProxyCommand::Set {
            mode,
            url,
            username,
            password_stdin,
            clear_password,
        } => {
            let password = if password_stdin {
                if std::io::stdin().is_terminal() {
                    bail!("Pipe the password from a protected source; interactive input may echo secrets");
                }
                let mut bytes = Vec::new();
                std::io::stdin().take(256).read_to_end(&mut bytes)?;
                if bytes.len() > 255 {
                    bail!("Password exceeds 255 bytes");
                }
                Some(String::from_utf8(bytes).context("Proxy password is not UTF-8")?)
            } else if clear_password {
                Some(String::new())
            } else {
                None
            };
            ProxySettings::save(
                &db,
                ProxySettingsInput {
                    mode: match mode {
                        Mode::Direct => ProxyMode::Direct,
                        Mode::System => ProxyMode::System,
                        Mode::Custom => ProxyMode::Custom,
                    },
                    url,
                    username,
                    password,
                },
            )?
        }
    };
    println!("{}", serde_json::to_string_pretty(&settings.view())?);
    Ok(())
}
