use anyhow::{bail, Result};
use clap::Subcommand;
use vibeshell_core::{
    ipc::{runtime_services::RuntimeRequest, IpcMessage},
    session::container::ContainerOpenRequest,
};

#[derive(Subcommand)]
pub enum ContainerCommand {
    /// Inspect containers without starting stopped containers or elevating.
    List {
        #[arg(long)]
        session: String,
    },
    /// Open a persistent container session, then use attach/ssh-session on its ID.
    Open {
        #[arg(long)]
        session: String,
        container: String,
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        workdir: Option<String>,
        #[arg(long, default_value = "/bin/sh")]
        shell: String,
    },
}

pub fn run(command: ContainerCommand) -> Result<()> {
    let resolve = |id: &str| {
        crate::session_alias::resolve(id).ok_or_else(|| anyhow::anyhow!("Unknown session alias"))
    };
    let request = match command {
        ContainerCommand::List { session } => RuntimeRequest::ContainerList {
            parent_session_id: resolve(&session)?,
        },
        ContainerCommand::Open {
            session,
            container,
            user,
            workdir,
            shell,
        } => RuntimeRequest::ContainerOpen {
            request: ContainerOpenRequest {
                parent_session_id: resolve(&session)?,
                container,
                user,
                workdir,
                shell,
            },
        },
    };
    match crate::ipc_support::send(&IpcMessage::SessionService { request })? {
        IpcMessage::ServiceResult { value } => {
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        IpcMessage::Error { message } => bail!("{message}"),
        _ => bail!("Update the VibeShell service to use container sessions"),
    }
}
