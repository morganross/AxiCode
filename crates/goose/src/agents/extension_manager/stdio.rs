use std::collections::HashMap;

use rmcp::transport::ConfigureCommandExt;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::super::container::Container;
use super::super::extension::{ExtensionError, ExtensionResult, ProcessExit};
use super::super::extension_malware_check;
use super::super::mcp_client::{ConnectContext, McpClient};
use crate::config::search_path::SearchPaths;
use crate::subprocess::spawn_long_lived_mcp_subprocess;

pub(super) async fn connect(
    cmd: &str,
    args: &[String],
    envs: HashMap<String, String>,
    container: Option<&Container>,
    mut ctx: ConnectContext,
) -> ExtensionResult<McpClient> {
    extension_malware_check::deny_if_malicious_cmd_args(cmd, args).await?;

    let command = match container {
        Some(container) => {
            ctx.docker_container = Some(container.id().to_string());
            tracing::info!(
                container = %container.id(),
                cmd = %cmd,
                "Starting stdio extension inside Docker container"
            );
            docker_exec(container, envs, cmd, args)
        }
        None => Command::new(resolve_command(cmd)).configure(|command| {
            command.args(args).envs(envs);
        }),
    };
    spawn(command, ctx).await
}

pub(super) fn docker_exec(
    container: &Container,
    envs: HashMap<String, String>,
    program: &str,
    args: &[String],
) -> Command {
    Command::new("docker").configure(|command| {
        command.arg("exec").arg("-i");
        for (key, value) in envs {
            command.arg("-e").arg(format!("{}={}", key, value));
        }
        command.arg(container.id()).arg(program).args(args);
    })
}

fn resolve_command(cmd: &str) -> std::path::PathBuf {
    SearchPaths::builder()
        .with_npm()
        .resolve(cmd)
        .unwrap_or_else(|_| {
            // let the OS raise the error
            std::path::PathBuf::from(cmd)
        })
}

pub(super) async fn spawn(mut command: Command, ctx: ConnectContext) -> ExtensionResult<McpClient> {
    if let Ok(path) = SearchPaths::builder().path() {
        command.env("PATH", path);
    }

    if ctx.working_dir.is_dir() {
        tracing::info!(
            "Setting MCP process working directory: {:?}",
            ctx.working_dir
        );
        command.current_dir(&ctx.working_dir);
    } else {
        tracing::warn!(
            "Working directory doesn't exist or isn't a directory: {:?}",
            ctx.working_dir
        );
    }

    let (transport, mut stderr) = spawn_long_lived_mcp_subprocess(command).await?;
    let mut stderr = stderr.take().ok_or_else(|| {
        ExtensionError::SetupError("failed to attach child process stderr".to_owned())
    })?;

    #[cfg(windows)]
    let startup_timeout = ctx
        .capabilities
        .host_info
        .as_ref()
        .filter(|host| host.client_name.as_deref() == Some("axiowl"))
        .map(|_| ctx.timeout.min(std::time::Duration::from_secs(45)));
    let stderr_task = tokio::spawn(async move {
        let mut all_stderr = Vec::new();
        stderr.read_to_end(&mut all_stderr).await?;
        Ok::<String, std::io::Error>(String::from_utf8_lossy(&all_stderr).into())
    });

    #[cfg(windows)]
    let stderr_abort = stderr_task.abort_handle();
    let setup = async move {
        match McpClient::connect(transport, ctx).await {
            Ok(client) => Ok(client),
            Err(error) => Err(match stderr_task.await? {
                Ok(stderr_content) => ProcessExit::new(stderr_content, error).into(),
                Err(e) => e.into(),
            }),
        }
    };
    #[cfg(windows)]
    if let Some(timeout) = startup_timeout {
        // The IDE allows 60 seconds for session setup, which awaits all MCP starts.
        // Bound both handshake and error collection without changing tool-call timeouts.
        return match tokio::time::timeout(timeout, setup).await {
            Ok(result) => result,
            Err(_) => {
                stderr_abort.abort();
                Err(ExtensionError::SetupError(format!(
                    "MCP stdio startup timed out after {} seconds",
                    timeout.as_secs()
                )))
            }
        };
    }
    setup.await
}
