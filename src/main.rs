mod agent;
mod git;
mod guest_binary;
mod instance;
mod queue;
mod worker;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use crate::queue::{Job, ReliableQueue};
use crate::worker::{Worker, WorkerConfig};

#[derive(Parser)]
#[command(name = "redis-agent-worker")]
#[command(about = "A reliable Redis-based agent worker for processing code changes", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Redis connection URL
    #[arg(
        long,
        env = "REDIS_URL",
        default_value = "redis://127.0.0.1:6379"
    )]
    redis_url: String,

    /// Queue name
    #[arg(long, env = "QUEUE_NAME", default_value = "agent_jobs")]
    queue_name: String,

    /// Instance allocator API URL
    #[arg(
        long,
        env = "ALLOCATOR_API_URL",
        default_value = "http://localhost:8080"
    )]
    allocator_api_url: String,

    /// Working directory for cloning repositories
    #[arg(long, env = "WORK_DIR", default_value = "/tmp/agent-worker")]
    work_dir: String,

    /// Log level
    #[arg(long, env = "LOG_LEVEL", default_value = "info")]
    log_level: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the worker to process jobs from the queue
    Run {
        /// Queue timeout in seconds for blocking operations
        #[arg(long, default_value = "30")]
        timeout: u64,
    },

    /// Enqueue a new job
    Enqueue {
        /// Unique job ID
        #[arg(long)]
        job_id: String,

        /// Repository URL
        #[arg(long)]
        repo_url: String,

        /// Branch name
        #[arg(long)]
        branch: String,

        /// Prompt for the agent
        #[arg(long)]
        prompt: String,

        /// Optional MCP connection URL
        #[arg(long)]
        mcp_connection_url: Option<String>,
    },

    /// Show queue statistics
    Stats {
        /// Queue timeout in seconds
        #[arg(long, default_value = "5")]
        timeout: u64,
    },

    /// Recover stalled jobs from processing queue
    Recover {
        /// Queue timeout in seconds
        #[arg(long, default_value = "5")]
        timeout: u64,
    },

    /// Peek at the next job without dequeuing
    Peek {
        /// Queue timeout in seconds
        #[arg(long, default_value = "5")]
        timeout: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let log_level = match cli.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .context("Failed to set tracing subscriber")?;

    match cli.command {
        Commands::Run { timeout } => {
            info!("Starting worker");
            let config = WorkerConfig {
                redis_url: cli.redis_url,
                queue_name: cli.queue_name,
                queue_timeout: timeout,
                allocator_api_url: cli.allocator_api_url,
                work_dir: cli.work_dir,
            };

            let mut worker = Worker::new(config).await?;
            worker.run().await?;
        }

        Commands::Enqueue {
            job_id,
            repo_url,
            branch,
            prompt,
            mcp_connection_url,
        } => {
            info!("Enqueueing job: {}", job_id);

            let mut queue =
                ReliableQueue::new(&cli.redis_url, &cli.queue_name, 5).await?;

            let job = Job {
                id: job_id,
                repo_url,
                branch,
                prompt,
                mcp_connection_url,
            };

            queue.enqueue(&job).await?;
            println!("Job enqueued successfully: {}", job.id);
        }

        Commands::Stats { timeout } => {
            let mut queue =
                ReliableQueue::new(&cli.redis_url, &cli.queue_name, timeout).await?;

            let queue_len = queue.len().await?;
            let processing_len = queue.processing_len().await?;

            println!("Queue Statistics:");
            println!("  Pending jobs: {}", queue_len);
            println!("  Processing jobs: {}", processing_len);
        }

        Commands::Recover { timeout } => {
            info!("Recovering stalled jobs");

            let mut queue =
                ReliableQueue::new(&cli.redis_url, &cli.queue_name, timeout).await?;

            let recovered = queue.recover_stalled_jobs().await?;
            println!("Recovered {} stalled jobs", recovered);
        }

        Commands::Peek { timeout } => {
            let mut queue =
                ReliableQueue::new(&cli.redis_url, &cli.queue_name, timeout).await?;

            match queue.peek().await? {
                Some(job) => {
                    println!("Next job in queue:");
                    println!("  ID: {}", job.id);
                    println!("  Repository: {}", job.repo_url);
                    println!("  Branch: {}", job.branch);
                    println!("  Prompt: {}", job.prompt);
                    if let Some(url) = job.mcp_connection_url {
                        println!("  MCP URL: {}", url);
                    }
                }
                None => {
                    println!("Queue is empty");
                }
            }
        }
    }

    Ok(())
}
