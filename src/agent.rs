use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub hyperlight_path: String,
    pub working_directory: String,
}

#[derive(Debug)]
pub struct AgentExecutor {
    config: AgentConfig,
}

impl AgentExecutor {
    pub fn new(config: AgentConfig) -> Self {
        Self { config }
    }

    /// Execute the agent with the given prompt in the repository
    /// The agent runs in Hyperlight with restricted permissions
    pub async fn execute(
        &self,
        repo_path: &Path,
        prompt: &str,
        mcp_connection_url: Option<&str>,
    ) -> Result<AgentResult> {
        info!("Executing agent in repository: {:?}", repo_path);
        debug!("Prompt: {}", prompt);

        // Prepare the agent command
        let mut cmd = Command::new(&self.config.hyperlight_path);
        cmd.current_dir(repo_path);

        // Set up environment variables for MCP connection
        if let Some(url) = mcp_connection_url {
            info!("Setting MCP connection URL: {}", url);
            cmd.env("MCP_CONNECTION_URL", url);

            // Configure network permissions to allow only MCP connection
            cmd.env("HYPERLIGHT_ALLOW_NETWORK", "mcp_only");
            cmd.env("HYPERLIGHT_MCP_URL", url);
        } else {
            // Completely disable network access if no MCP URL provided
            cmd.env("HYPERLIGHT_ALLOW_NETWORK", "false");
        }

        // Set working directory permissions
        cmd.env("HYPERLIGHT_WORKING_DIR", repo_path.to_str().unwrap());
        cmd.env("HYPERLIGHT_ALLOW_FILE_WRITE", "true");
        cmd.env("HYPERLIGHT_ALLOW_FILE_READ", "true");

        // Pass the prompt as argument or stdin
        cmd.arg("--prompt").arg(prompt);

        // Configure command to capture output
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        info!("Starting agent process");
        let mut child = cmd.spawn().context("Failed to spawn agent process")?;

        // Capture stdout
        let stdout = child.stdout.take().context("Failed to capture stdout")?;
        let stdout_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut output = Vec::new();

            while let Ok(Some(line)) = lines.next_line().await {
                info!("[Agent stdout] {}", line);
                output.push(line);
            }

            output
        });

        // Capture stderr
        let stderr = child.stderr.take().context("Failed to capture stderr")?;
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let mut output = Vec::new();

            while let Ok(Some(line)) = lines.next_line().await {
                warn!("[Agent stderr] {}", line);
                output.push(line);
            }

            output
        });

        // Wait for process to complete
        let status = child.wait().await.context("Failed to wait for agent process")?;

        // Collect output
        let stdout_lines = stdout_handle.await.context("Failed to join stdout task")?;
        let stderr_lines = stderr_handle.await.context("Failed to join stderr task")?;

        let result = AgentResult {
            success: status.success(),
            exit_code: status.code().unwrap_or(-1),
            stdout: stdout_lines.join("\n"),
            stderr: stderr_lines.join("\n"),
        };

        if result.success {
            info!("Agent execution completed successfully");
        } else {
            error!("Agent execution failed with exit code: {}", result.exit_code);
        }

        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub struct AgentResult {
    pub success: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl AgentResult {
    pub fn is_success(&self) -> bool {
        self.success
    }
}
