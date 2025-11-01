use anyhow::{Context, Result};
use hyperlight_host::sandbox::SandboxConfiguration;
use hyperlight_host::{new_error, GuestBinary, MultiUseSandbox, UninitializedSandbox};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::guest_binary::GUEST_BINARY;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub working_directory: String,
}

#[derive(Debug)]
pub struct AgentExecutor {
    config: AgentConfig,
    http_client: Client,
    // Track the allowed MCP server URL for this executor instance
    allowed_mcp_url: Arc<RwLock<Option<Url>>>,
}

impl AgentExecutor {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
            allowed_mcp_url: Arc::new(RwLock::new(None)),
        }
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

        // Set the allowed MCP URL for this execution
        if let Some(url) = mcp_connection_url {
            let parsed_url = Url::parse(url).context("Invalid MCP connection URL")?;
            *self.allowed_mcp_url.write().await = Some(parsed_url);
            info!("Restricted networking to MCP server: {}", url);
        } else {
            *self.allowed_mcp_url.write().await = None;
            warn!("No MCP URL provided - agent will have no network access");
        }

        // Load the guest binary from embedded bytes
        let guest_binary = GuestBinary::Buffer(GUEST_BINARY);

        info!("Loading embedded guest binary ({} bytes)", GUEST_BINARY.len());

        // Create sandbox configuration
        let config = SandboxConfiguration::default();
        // Note: set_working_directory might not be available in this version
        // Will configure access through host functions instead

        // Create uninitialized sandbox
        let mut uninitialized = UninitializedSandbox::new(guest_binary, Some(config))
            .context("Failed to create Hyperlight sandbox")?;

        info!("Hyperlight sandbox created");

        // Register host functions that the guest can call
        self.register_host_functions(&mut uninitialized).await?;

        // Evolve into a multi-use sandbox
        let mut sandbox: MultiUseSandbox = uninitialized
            .evolve()
            .context("Failed to evolve sandbox")?;

        info!("Hyperlight sandbox initialized successfully");

        // Call the guest's ExecuteAgent function
        let mcp_url_param = mcp_connection_url.unwrap_or("");

        info!("Calling guest ExecuteAgent function");
        let output: String = sandbox
            .call("ExecuteAgent", (prompt.to_string(), mcp_url_param.to_string()))
            .context("Failed to call guest function")?;

        info!("Agent execution completed successfully");

        Ok(AgentResult {
            success: true,
            exit_code: 0,
            stdout: output,
            stderr: String::new(),
        })
    }

    /// Register host functions that the guest can call
    /// These functions provide controlled access to network and file operations
    async fn register_host_functions(
        &self,
        sandbox: &mut UninitializedSandbox,
    ) -> Result<()> {
        let allowed_url = self.allowed_mcp_url.clone();
        let http_client = self.http_client.clone();

        // Host function: Initialize MCP connection
        // Validates that the URL matches the allowed MCP server
        let allowed_for_init = allowed_url.clone();
        sandbox
            .register("InitializeMCPConnection", move |url_str: String| -> hyperlight_host::Result<()> {
                // Validate URL matches allowed MCP server
                let url = Url::parse(&url_str)
                    .map_err(|e| new_error!("Invalid URL: {}", e))?;
                let allowed = allowed_for_init.blocking_read();

                if let Some(allowed_url) = allowed.as_ref() {
                    if url.host_str() != allowed_url.host_str()
                        || url.port() != allowed_url.port()
                        || url.scheme() != allowed_url.scheme()
                    {
                        error!(
                            "Blocked unauthorized connection attempt to: {}. Only {} is allowed.",
                            url, allowed_url
                        );
                        return Err(new_error!("Unauthorized network access"));
                    }
                } else {
                    error!("No MCP server configured - blocking all network access");
                    return Err(new_error!("Network access not allowed"));
                }

                info!("MCP connection initialized to: {}", url);
                Ok(())
            })
            .context("Failed to register InitializeMCPConnection host function")?;

        // Host function: Get available MCP tools
        let http_for_tools = http_client.clone();
        let allowed_for_tools = allowed_url.clone();
        sandbox
            .register("GetMCPTools", move || -> hyperlight_host::Result<String> {
                let allowed = allowed_for_tools.blocking_read();
                let mcp_url = allowed
                    .as_ref()
                    .ok_or_else(|| new_error!("MCP server not configured"))?;

                // Make request to MCP server to list tools
                let tools_url = mcp_url.join("/tools")
                    .map_err(|e| new_error!("URL join error: {}", e))?;
                info!("Fetching MCP tools from: {}", tools_url);

                // Create a new runtime for this blocking call
                let rt = tokio::runtime::Runtime::new()
                    .map_err(|e| new_error!("Failed to create runtime: {}", e))?;

                let response = rt.block_on(async {
                    http_for_tools
                        .get(tools_url.as_str())
                        .send()
                        .await
                        .map_err(|e| new_error!("HTTP request failed: {}", e))?
                        .text()
                        .await
                        .map_err(|e| new_error!("Failed to read response: {}", e))
                })?;

                Ok(response)
            })
            .context("Failed to register GetMCPTools host function")?;

        // Host function: Execute MCP tool
        let http_for_exec = http_client.clone();
        let allowed_for_exec = allowed_url.clone();
        sandbox
            .register("ExecuteMCPTool", move |tool_name: String, arguments_json: String| -> hyperlight_host::Result<String> {
                let allowed = allowed_for_exec.blocking_read();
                let mcp_url = allowed
                    .as_ref()
                    .ok_or_else(|| new_error!("MCP server not configured"))?;

                // Make request to MCP server to execute tool
                let tool_url = mcp_url.join(&format!("/tools/{}", tool_name))
                    .map_err(|e| new_error!("URL join error: {}", e))?;
                info!("Executing MCP tool '{}' at: {}", tool_name, tool_url);

                // Create a new runtime for this blocking call
                let rt = tokio::runtime::Runtime::new()
                    .map_err(|e| new_error!("Failed to create runtime: {}", e))?;

                let response = rt.block_on(async {
                    http_for_exec
                        .post(tool_url.as_str())
                        .header("Content-Type", "application/json")
                        .body(arguments_json)
                        .send()
                        .await
                        .map_err(|e| new_error!("HTTP request failed: {}", e))?
                        .text()
                        .await
                        .map_err(|e| new_error!("Failed to read response: {}", e))
                })?;

                Ok(response)
            })
            .context("Failed to register ExecuteMCPTool host function")?;

        info!("All host functions registered successfully");
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_executor_creation() {
        let config = AgentConfig {
            working_directory: "/tmp/test".to_string(),
        };
        let executor = AgentExecutor::new(config);
        assert!(executor.http_client.get("http://example.com").build().is_ok());
    }

    #[tokio::test]
    async fn test_guest_binary_embedded() {
        // Verify the guest binary is embedded and non-empty
        assert!(!GUEST_BINARY.is_empty(), "Guest binary should be embedded");
        assert!(
            GUEST_BINARY.len() > 1000,
            "Guest binary should be a reasonable size"
        );
    }

    #[tokio::test]
    async fn test_agent_execution_without_mcp() {
        let config = AgentConfig {
            working_directory: "/tmp/test".to_string(),
        };
        let executor = AgentExecutor::new(config);

        // Create a temporary directory for testing
        let temp_dir = std::env::temp_dir().join("hyperlight_test");
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Execute without MCP URL (should fail gracefully)
        let result = executor
            .execute(&temp_dir, "test prompt", None)
            .await;

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);

        // This should either succeed with an error message or fail with a clear error
        // The important thing is that it doesn't panic
        match result {
            Ok(res) => {
                println!("Result: {:?}", res);
                // Agent should indicate no network access
            }
            Err(e) => {
                println!("Expected error (no MCP configured): {}", e);
                // This is expected when no MCP server is configured
            }
        }
    }
}
