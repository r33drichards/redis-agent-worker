use anyhow::{Context, Result};
use redis::{aio::ConnectionManager, AsyncCommands};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub repo_url: String,
    pub branch: String,
    pub prompt: String,
    pub mcp_connection_url: Option<String>,
}

pub struct ReliableQueue {
    connection: ConnectionManager,
    queue_name: String,
    processing_queue_name: String,
    timeout_seconds: u64,
}

impl ReliableQueue {
    pub async fn new(
        redis_url: &str,
        queue_name: &str,
        timeout_seconds: u64,
    ) -> Result<Self> {
        let client = redis::Client::open(redis_url)
            .context("Failed to create Redis client")?;
        let connection = ConnectionManager::new(client)
            .await
            .context("Failed to connect to Redis")?;

        Ok(Self {
            connection,
            queue_name: queue_name.to_string(),
            processing_queue_name: format!("{}_processing", queue_name),
            timeout_seconds,
        })
    }

    /// Reliably dequeue a job using RPOPLPUSH pattern
    /// This moves the job from the main queue to a processing queue
    pub async fn dequeue(&mut self) -> Result<Option<Job>> {
        debug!("Attempting to dequeue job from {}", self.queue_name);

        // Use BRPOPLPUSH for blocking reliable dequeue
        let result: Option<String> = self
            .connection
            .brpoplpush(
                &self.queue_name,
                &self.processing_queue_name,
                self.timeout_seconds as f64,
            )
            .await
            .context("Failed to execute BRPOPLPUSH")?;

        match result {
            Some(job_json) => {
                debug!("Dequeued job: {}", job_json);
                let job: Job = serde_json::from_str(&job_json)
                    .context("Failed to deserialize job")?;
                info!("Successfully dequeued job: {}", job.id);
                Ok(Some(job))
            }
            None => {
                debug!("No job available in queue");
                Ok(None)
            }
        }
    }

    /// Enqueue a job to the main queue
    pub async fn enqueue(&mut self, job: &Job) -> Result<()> {
        let job_json = serde_json::to_string(job)
            .context("Failed to serialize job")?;

        self.connection
            .lpush::<_, _, ()>(&self.queue_name, &job_json)
            .await
            .context("Failed to enqueue job")?;

        info!("Enqueued job: {}", job.id);
        Ok(())
    }

    /// Acknowledge successful job processing by removing from processing queue
    pub async fn ack(&mut self, job: &Job) -> Result<()> {
        let job_json = serde_json::to_string(job)
            .context("Failed to serialize job")?;

        let removed: i32 = self
            .connection
            .lrem(&self.processing_queue_name, 1, &job_json)
            .await
            .context("Failed to remove job from processing queue")?;

        if removed > 0 {
            info!("Successfully acknowledged job: {}", job.id);
        } else {
            warn!("Job not found in processing queue: {}", job.id);
        }

        Ok(())
    }

    /// Move a failed job back to the main queue for retry
    pub async fn nack(&mut self, job: &Job) -> Result<()> {
        let job_json = serde_json::to_string(job)
            .context("Failed to serialize job")?;

        // Remove from processing queue
        let removed: i32 = self
            .connection
            .lrem(&self.processing_queue_name, 1, &job_json)
            .await
            .context("Failed to remove job from processing queue")?;

        if removed > 0 {
            // Re-enqueue to main queue
            self.connection
                .lpush::<_, _, ()>(&self.queue_name, &job_json)
                .await
                .context("Failed to re-enqueue job")?;

            warn!("Job moved back to main queue for retry: {}", job.id);
        } else {
            error!("Job not found in processing queue during NACK: {}", job.id);
        }

        Ok(())
    }

    /// Recover jobs from processing queue (e.g., after a crash)
    pub async fn recover_stalled_jobs(&mut self) -> Result<usize> {
        info!("Recovering stalled jobs from processing queue");

        let mut recovered = 0;
        loop {
            let job_json: Option<String> = self
                .connection
                .rpoplpush(&self.processing_queue_name, &self.queue_name)
                .await
                .context("Failed to recover job")?;

            match job_json {
                Some(_) => recovered += 1,
                None => break,
            }
        }

        if recovered > 0 {
            info!("Recovered {} stalled jobs", recovered);
        } else {
            debug!("No stalled jobs to recover");
        }

        Ok(recovered)
    }

    /// Peek at the next job without dequeuing
    pub async fn peek(&mut self) -> Result<Option<Job>> {
        let result: Option<String> = self
            .connection
            .lindex(&self.queue_name, -1)
            .await
            .context("Failed to peek at queue")?;

        match result {
            Some(job_json) => {
                let job: Job = serde_json::from_str(&job_json)
                    .context("Failed to deserialize job")?;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    /// Get queue length
    pub async fn len(&mut self) -> Result<usize> {
        let len: usize = self
            .connection
            .llen(&self.queue_name)
            .await
            .context("Failed to get queue length")?;
        Ok(len)
    }

    /// Get processing queue length
    pub async fn processing_len(&mut self) -> Result<usize> {
        let len: usize = self
            .connection
            .llen(&self.processing_queue_name)
            .await
            .context("Failed to get processing queue length")?;
        Ok(len)
    }
}
