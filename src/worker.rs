use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::{error, info, warn};

use crate::agent::{AgentConfig, AgentExecutor};
use crate::git::GitRepo;
use crate::instance::{InstanceAllocator, InstanceGuard};
use crate::queue::{Job, ReliableQueue};

pub struct WorkerConfig {
    pub redis_url: String,
    pub queue_name: String,
    pub queue_timeout: u64,
    pub allocator_api_url: String,
    pub work_dir: String,
}

pub struct Worker {
    queue: ReliableQueue,
    allocator: InstanceAllocator,
    agent_executor: AgentExecutor,
    work_dir: PathBuf,
}

impl Worker {
    pub async fn new(config: WorkerConfig) -> Result<Self> {
        info!("Initializing worker");

        let queue = ReliableQueue::new(
            &config.redis_url,
            &config.queue_name,
            config.queue_timeout,
        )
        .await
        .context("Failed to create queue")?;

        let allocator = InstanceAllocator::new(config.allocator_api_url);

        let agent_config = AgentConfig {
            working_directory: config.work_dir.clone(),
        };
        let agent_executor = AgentExecutor::new(agent_config);

        let work_dir = PathBuf::from(config.work_dir);
        std::fs::create_dir_all(&work_dir)
            .context("Failed to create work directory")?;

        info!("Worker initialized successfully");

        Ok(Self {
            queue,
            allocator,
            agent_executor,
            work_dir,
        })
    }

    /// Run the worker loop
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting worker loop");

        // Recover any stalled jobs on startup
        self.queue.recover_stalled_jobs().await?;

        loop {
            match self.process_next_job().await {
                Ok(processed) => {
                    if !processed {
                        info!("No jobs available, waiting...");
                    }
                }
                Err(e) => {
                    error!("Error processing job: {:#}", e);
                    // Continue processing other jobs even if one fails
                }
            }
        }
    }

    /// Process the next job from the queue
    async fn process_next_job(&mut self) -> Result<bool> {
        // Dequeue a job
        let job = match self.queue.dequeue().await? {
            Some(job) => job,
            None => return Ok(false),
        };

        info!("Processing job: {}", job.id);

        // Process the job and handle result
        match self.process_job(&job).await {
            Ok(_) => {
                info!("Job completed successfully: {}", job.id);
                self.queue.ack(&job).await?;
            }
            Err(e) => {
                error!("Job failed: {} - {:#}", job.id, e);
                // Move job back to queue for retry
                self.queue.nack(&job).await?;
            }
        }

        Ok(true)
    }

    /// Process a single job
    async fn process_job(&self, job: &Job) -> Result<()> {
        info!("Starting job processing: {}", job.id);

        // Step 1: Borrow an instance
        info!("Borrowing instance for job: {}", job.id);
        let instance = self.allocator.borrow_instance().await?;
        let instance_guard = InstanceGuard::new(instance, self.allocator.clone());

        // Step 2: Clone repository
        let repo_dir = self.work_dir.join(&job.id);
        if repo_dir.exists() {
            info!("Cleaning up existing repository directory");
            std::fs::remove_dir_all(&repo_dir)
                .context("Failed to remove existing repo directory")?;
        }

        info!("Cloning repository: {}", job.repo_url);
        let git_repo = GitRepo::clone(&job.repo_url, &repo_dir)
            .context("Failed to clone repository")?;

        // Step 3: Checkout branch
        info!("Checking out branch: {}", job.branch);
        git_repo.fetch().context("Failed to fetch from remote")?;
        git_repo
            .checkout_branch(&job.branch)
            .context("Failed to checkout branch")?;

        // Step 4: Execute agent with MCP permissions
        info!("Executing agent for job: {}", job.id);
        let mcp_url = job
            .mcp_connection_url
            .as_deref()
            .or(Some(&instance_guard.instance().mcp_connection_url));

        let result = self
            .agent_executor
            .execute(git_repo.path(), &job.prompt, mcp_url)
            .await
            .context("Failed to execute agent")?;

        if !result.is_success() {
            anyhow::bail!(
                "Agent execution failed with exit code {}: {}",
                result.exit_code,
                result.stderr
            );
        }

        // Step 5: Check for changes and commit/push if needed
        if git_repo.has_changes()? {
            info!("Changes detected, committing and pushing");

            git_repo.stage_all().context("Failed to stage changes")?;

            let commit_message = format!(
                "Agent changes for job: {}\n\nPrompt: {}",
                job.id, job.prompt
            );
            git_repo
                .commit(&commit_message)
                .context("Failed to commit changes")?;

            git_repo
                .push(&job.branch)
                .context("Failed to push changes")?;

            info!("Changes successfully pushed to branch: {}", job.branch);
        } else {
            warn!("No changes detected after agent execution");
        }

        // Step 6: Clean up repository
        info!("Cleaning up repository directory");
        std::fs::remove_dir_all(&repo_dir)
            .context("Failed to remove repo directory")?;

        // Step 7: Return instance (automatically handled by InstanceGuard drop)
        instance_guard.return_instance().await?;

        info!("Job processing completed: {}", job.id);
        Ok(())
    }

    /// Get queue statistics
    pub async fn get_stats(&mut self) -> Result<WorkerStats> {
        let queue_len = self.queue.len().await?;
        let processing_len = self.queue.processing_len().await?;

        Ok(WorkerStats {
            queue_length: queue_len,
            processing_length: processing_len,
        })
    }
}

#[derive(Debug)]
pub struct WorkerStats {
    pub queue_length: usize,
    pub processing_length: usize,
}
