use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub mcp_connection_url: String,
    pub api_url: String,
}

#[derive(Clone)]
pub struct InstanceAllocator {
    allocator_api_url: String,
    client: reqwest::Client,
}

impl InstanceAllocator {
    pub fn new(allocator_api_url: String) -> Self {
        Self {
            allocator_api_url,
            client: reqwest::Client::new(),
        }
    }

    /// Borrow an instance from the allocator
    pub async fn borrow_instance(&self) -> Result<Instance> {
        info!("Requesting instance from allocator");

        let url = format!("{}/borrow", self.allocator_api_url);
        let response = self
            .client
            .post(&url)
            .send()
            .await
            .context("Failed to send borrow request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to borrow instance: {} - {}", status, body);
        }

        let instance: Instance = response
            .json()
            .await
            .context("Failed to parse instance response")?;

        info!("Successfully borrowed instance: {}", instance.id);
        debug!("Instance details: {:?}", instance);

        Ok(instance)
    }

    /// Return an instance to the allocator
    pub async fn return_instance(&self, instance: &Instance) -> Result<()> {
        info!("Returning instance: {}", instance.id);

        let url = format!("{}/return", self.allocator_api_url);
        let response = self
            .client
            .post(&url)
            .json(instance)
            .send()
            .await
            .context("Failed to send return request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to return instance: {} - {}", status, body);
        }

        info!("Successfully returned instance: {}", instance.id);

        Ok(())
    }
}

/// RAII guard for automatic instance return
pub struct InstanceGuard {
    instance: Option<Instance>,
    allocator: InstanceAllocator,
}

impl InstanceGuard {
    pub fn new(instance: Instance, allocator: InstanceAllocator) -> Self {
        Self {
            instance: Some(instance),
            allocator,
        }
    }

    pub fn instance(&self) -> &Instance {
        self.instance.as_ref().unwrap()
    }

    /// Manually return the instance
    pub async fn return_instance(mut self) -> Result<()> {
        if let Some(instance) = self.instance.take() {
            self.allocator.return_instance(&instance).await?;
        }
        Ok(())
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        if let Some(instance) = &self.instance {
            // Try to return the instance even on panic
            // We can't make this async in Drop, so we spawn a blocking task
            let instance = instance.clone();
            let allocator_url = self.allocator.allocator_api_url.clone();

            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let allocator = InstanceAllocator::new(allocator_url);
                    if let Err(e) = allocator.return_instance(&instance).await {
                        eprintln!("Failed to return instance in Drop: {}", e);
                    }
                });
            });
        }
    }
}
