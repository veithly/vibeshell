use crate::{ipc_support, session_alias};
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::Value;
use std::collections::BTreeMap;
use vibeshell_core::{ipc::IpcMessage, plugins::PluginExecuteRequest};

#[derive(Subcommand)]
pub enum PluginCommand {
    /// Discover plugins and their current installed/enabled state.
    List {
        #[arg(long)]
        installed: bool,
        #[arg(long)]
        json: bool,
    },
    /// Read action schemas as JSON; never returns stored settings or secrets.
    Describe { plugin_id: String },
    /// Read the current plugin reference as Markdown, including imported plugins.
    Docs { plugin_id: String },
    /// Execute one installed, enabled plugin action on an existing session.
    Run {
        plugin_id: String,
        action_id: String,
        #[arg(long)]
        session: String,
        /// JSON input object; get the exact schema from plugins describe.
        #[arg(long, default_value = "{}", value_parser = parse_inputs)]
        inputs: BTreeMap<String, Value>,
        /// Use only after the user approved this exact action and target.
        #[arg(long)]
        confirm: bool,
        /// Explicit opt-in for actions that permit sudo; requires --confirm.
        #[arg(long = "sudo", requires = "confirm")]
        try_sudo: bool,
    },
}

fn parse_inputs(text: &str) -> Result<BTreeMap<String, Value>, String> {
    if text.len() > 16 * 1024 {
        return Err("Plugin input object exceeds 16 KiB".into());
    }
    serde_json::from_str(text).map_err(|_| "--inputs must be a valid JSON object".into())
}

fn response(message: IpcMessage) -> Result<Value> {
    match ipc_support::send(&message)? {
        IpcMessage::PluginData { data } => Ok(data),
        IpcMessage::Error { message } => bail!("{message}"),
        _ => bail!("The running VibeShell service does not support this plugin API. Upgrade/restart the service without discarding active work."),
    }
}

pub fn run(command: PluginCommand) -> Result<()> {
    let data = match command {
        PluginCommand::List { installed, json } => {
            let data = response(IpcMessage::PluginList {
                installed_only: installed,
            })?;
            if !json {
                for plugin in data.as_array().context("Invalid plugin list response")? {
                    println!(
                        "{}\tinstalled={}\tenabled={}\t{}",
                        plugin["id"].as_str().unwrap_or_default(),
                        plugin["installed"],
                        plugin["enabled"],
                        plugin["reference"].as_str().unwrap_or_default()
                    );
                }
                return Ok(());
            }
            data
        }
        PluginCommand::Describe { plugin_id } => response(IpcMessage::PluginDescribe {
            plugin_id,
            reference: false,
        })?,
        PluginCommand::Docs { plugin_id } => {
            let data = response(IpcMessage::PluginDescribe {
                plugin_id,
                reference: true,
            })?;
            println!(
                "{}",
                data.as_str().context("Invalid plugin reference response")?
            );
            return Ok(());
        }
        PluginCommand::Run {
            plugin_id,
            action_id,
            session,
            inputs,
            confirm,
            try_sudo,
        } => {
            let session_id = session_alias::resolve(&session).unwrap_or(session);
            response(IpcMessage::PluginExecute {
                request: PluginExecuteRequest {
                    plugin_id,
                    action_id,
                    session_id,
                    inputs,
                    confirmed: confirm,
                    try_sudo,
                    sudo_password: None,
                },
            })?
        }
    };
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inputs_are_bounded_objects_not_arbitrary_json() {
        assert!(parse_inputs("[]").is_err());
        assert!(parse_inputs(&" ".repeat(16 * 1024 + 1)).is_err());
        let input = parse_inputs(r#"{"count":3,"all":true,"name":"a 'quoted' value"}"#).unwrap();
        assert_eq!(input["count"], 3);
        assert_eq!(input["all"], true);
    }
}
