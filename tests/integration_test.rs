mod common;

use anyhow::Result;
use redis_agent_worker::queue::{Job, ReliableQueue};
use std::time::Duration;
use tempfile::TempDir;
use testcontainers::{runners::AsyncRunner, GenericImage};
use uuid::Uuid;

#[tokio::test]
async fn test_queue_enqueue_dequeue() -> Result<()> {
    common::init_test_logging();

    // Start Redis container
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    // Create queue
    let mut queue = ReliableQueue::new(&redis_url, "test_queue", 5).await?;

    // Create a test job
    let job = Job {
        id: Uuid::new_v4().to_string(),
        repo_url: "git@github.com:test/repo.git".to_string(),
        branch: "main".to_string(),
        prompt: "Test prompt".to_string(),
        mcp_connection_url: Some("http://mcp.example.com".to_string()),
    };

    // Enqueue the job
    queue.enqueue(&job).await?;

    // Check queue length
    let len = queue.len().await?;
    assert_eq!(len, 1, "Queue should have 1 job");

    // Peek at the job
    let peeked = queue.peek().await?;
    assert!(peeked.is_some(), "Peek should return the job");
    assert_eq!(peeked.unwrap().id, job.id, "Peeked job ID should match");

    // Dequeue the job
    let dequeued = queue.dequeue().await?;
    assert!(dequeued.is_some(), "Dequeue should return the job");
    assert_eq!(dequeued.unwrap().id, job.id, "Dequeued job ID should match");

    // Queue should be empty now, but processing queue has 1
    let len = queue.len().await?;
    assert_eq!(len, 0, "Main queue should be empty");

    let processing_len = queue.processing_len().await?;
    assert_eq!(processing_len, 1, "Processing queue should have 1 job");

    // Acknowledge the job
    queue.ack(&job).await?;

    let processing_len = queue.processing_len().await?;
    assert_eq!(processing_len, 0, "Processing queue should be empty after ACK");

    Ok(())
}

#[tokio::test]
async fn test_queue_nack_retry() -> Result<()> {
    common::init_test_logging();

    // Start Redis container
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    // Create queue
    let mut queue = ReliableQueue::new(&redis_url, "test_nack_queue", 5).await?;

    // Create a test job
    let job = Job {
        id: Uuid::new_v4().to_string(),
        repo_url: "git@github.com:test/repo.git".to_string(),
        branch: "main".to_string(),
        prompt: "Test prompt".to_string(),
        mcp_connection_url: None,
    };

    // Enqueue the job
    queue.enqueue(&job).await?;

    // Dequeue the job
    let dequeued = queue.dequeue().await?;
    assert!(dequeued.is_some(), "Should dequeue job");

    // NACK the job (simulating failure)
    queue.nack(&job).await?;

    // Job should be back in the main queue
    let len = queue.len().await?;
    assert_eq!(len, 1, "Job should be back in main queue");

    let processing_len = queue.processing_len().await?;
    assert_eq!(processing_len, 0, "Processing queue should be empty");

    // Should be able to dequeue again
    let dequeued_again = queue.dequeue().await?;
    assert!(dequeued_again.is_some(), "Should be able to dequeue job again");
    assert_eq!(dequeued_again.unwrap().id, job.id, "Should get the same job");

    Ok(())
}

#[tokio::test]
async fn test_queue_recovery() -> Result<()> {
    common::init_test_logging();

    // Start Redis container
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    // Create queue
    let mut queue = ReliableQueue::new(&redis_url, "test_recovery_queue", 5).await?;

    // Create multiple test jobs
    let jobs: Vec<Job> = (0..3)
        .map(|i| Job {
            id: format!("job-{}", i),
            repo_url: "git@github.com:test/repo.git".to_string(),
            branch: "main".to_string(),
            prompt: format!("Test prompt {}", i),
            mcp_connection_url: None,
        })
        .collect();

    // Enqueue all jobs
    for job in &jobs {
        queue.enqueue(job).await?;
    }

    // Dequeue all jobs (simulating worker processing)
    for _ in 0..3 {
        queue.dequeue().await?;
    }

    // Now all jobs are in processing queue
    let processing_len = queue.processing_len().await?;
    assert_eq!(processing_len, 3, "All jobs should be in processing queue");

    let main_len = queue.len().await?;
    assert_eq!(main_len, 0, "Main queue should be empty");

    // Simulate worker crash and recovery
    let recovered = queue.recover_stalled_jobs().await?;
    assert_eq!(recovered, 3, "Should recover 3 jobs");

    // All jobs should be back in main queue
    let main_len = queue.len().await?;
    assert_eq!(main_len, 3, "All jobs should be back in main queue");

    let processing_len = queue.processing_len().await?;
    assert_eq!(processing_len, 0, "Processing queue should be empty");

    Ok(())
}

#[tokio::test]
async fn test_instance_allocator() -> Result<()> {
    common::init_test_logging();

    // Start mock allocator
    let (allocator_url, state) = common::start_mock_allocator().await;

    // Test borrowing an instance
    use redis_agent_worker::instance::InstanceAllocator;
    let allocator = InstanceAllocator::new(allocator_url.clone());

    let instance1 = allocator.borrow_instance().await?;
    assert!(instance1.id.starts_with("mock-instance-"));
    assert!(instance1.mcp_connection_url.starts_with("http://mock-mcp-"));

    // Check state
    let borrow_count = state.borrow_count().await;
    assert_eq!(borrow_count, 1, "Should have 1 borrowed instance");

    // Return the instance
    allocator.return_instance(&instance1).await?;

    let return_count = state.return_count().await;
    assert_eq!(return_count, 1, "Should have 1 returned instance");

    // Borrow multiple instances
    let instance2 = allocator.borrow_instance().await?;
    let instance3 = allocator.borrow_instance().await?;

    assert_ne!(instance2.id, instance3.id, "Instances should have unique IDs");

    let borrow_count = state.borrow_count().await;
    assert_eq!(borrow_count, 3, "Should have 3 total borrowed instances");

    Ok(())
}

#[tokio::test]
async fn test_git_operations() -> Result<()> {
    common::init_test_logging();

    let temp_dir = TempDir::new()?;
    let branch_name = "test-branch";

    // Setup git environment with remote
    let (local_path, remote_url) = common::setup_test_git_env(temp_dir.path(), branch_name)?;

    // Clone the repository to a new location
    let clone_dir = temp_dir.path().join("cloned");
    use redis_agent_worker::git::GitRepo;
    let git_repo = GitRepo::clone(&remote_url, &clone_dir)?;

    // Checkout the test branch
    git_repo.fetch()?;
    git_repo.checkout_branch(branch_name)?;

    // Make a change
    let test_file = clone_dir.join("test.txt");
    std::fs::write(&test_file, "Test content\n")?;

    // Check that there are changes
    assert!(git_repo.has_changes()?, "Should detect changes");

    // Commit and push
    git_repo.stage_all()?;
    git_repo.commit("Add test file")?;
    git_repo.push(branch_name)?;

    // Verify no more changes
    assert!(!git_repo.has_changes()?, "Should have no changes after commit");

    Ok(())
}

#[tokio::test]
async fn test_full_workflow_with_mock_agent() -> Result<()> {
    common::init_test_logging();

    // Setup Redis
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    // Setup mock allocator
    let (allocator_url, allocator_state) = common::start_mock_allocator().await;

    // Setup git environment
    let temp_dir = TempDir::new()?;
    let branch_name = "feature-test";
    let (_, remote_url) = common::setup_test_git_env(temp_dir.path(), branch_name)?;

    // Create work directory
    let work_dir = temp_dir.path().join("work");
    std::fs::create_dir_all(&work_dir)?;

    // Create a test job
    let job = Job {
        id: Uuid::new_v4().to_string(),
        repo_url: remote_url.clone(),
        branch: branch_name.to_string(),
        prompt: "Add a new feature".to_string(),
        mcp_connection_url: None,
    };

    // Enqueue the job
    let mut queue = ReliableQueue::new(&redis_url, "integration_test_queue", 5).await?;
    queue.enqueue(&job).await?;

    // Verify job is in queue
    let queue_len = queue.len().await?;
    assert_eq!(queue_len, 1, "Job should be in queue");

    // Dequeue the job (simulating worker)
    let dequeued_job = queue.dequeue().await?.expect("Should dequeue job");
    assert_eq!(dequeued_job.id, job.id);

    // Verify job moved to processing queue
    let processing_len = queue.processing_len().await?;
    assert_eq!(processing_len, 1, "Job should be in processing queue");

    // Simulate worker borrowing an instance
    use redis_agent_worker::instance::InstanceAllocator;
    let allocator = InstanceAllocator::new(allocator_url);
    let instance = allocator.borrow_instance().await?;

    assert_eq!(allocator_state.borrow_count().await, 1);

    // Simulate cloning and working with the repository
    let job_work_dir = work_dir.join(&dequeued_job.id);
    use redis_agent_worker::git::GitRepo;
    let git_repo = GitRepo::clone(&dequeued_job.repo_url, &job_work_dir)?;
    git_repo.fetch()?;
    git_repo.checkout_branch(&dequeued_job.branch)?;

    // Simulate agent making changes
    let feature_file = job_work_dir.join("feature.txt");
    std::fs::write(&feature_file, "New feature implementation\n")?;

    // Commit and push changes
    git_repo.stage_all()?;
    git_repo.commit(&format!("Implement feature for job {}", dequeued_job.id))?;
    git_repo.push(&dequeued_job.branch)?;

    // Return the instance
    allocator.return_instance(&instance).await?;
    assert_eq!(allocator_state.return_count().await, 1);

    // Acknowledge the job
    queue.ack(&dequeued_job).await?;

    // Verify queues are empty
    assert_eq!(queue.len().await?, 0, "Main queue should be empty");
    assert_eq!(queue.processing_len().await?, 0, "Processing queue should be empty");

    // Verify the changes were pushed by cloning again
    let verify_dir = temp_dir.path().join("verify");
    let verify_repo = GitRepo::clone(&remote_url, &verify_dir)?;
    verify_repo.fetch()?;
    verify_repo.checkout_branch(branch_name)?;

    let feature_file_verify = verify_dir.join("feature.txt");
    assert!(feature_file_verify.exists(), "Feature file should exist in remote");

    let content = std::fs::read_to_string(&feature_file_verify)?;
    assert_eq!(content, "New feature implementation\n");

    Ok(())
}

#[tokio::test]
async fn test_blocking_queue_timeout() -> Result<()> {
    common::init_test_logging();

    // Start Redis container
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    // Create queue with short timeout
    let mut queue = ReliableQueue::new(&redis_url, "test_timeout_queue", 2).await?;

    // Try to dequeue from empty queue (should timeout)
    let start = std::time::Instant::now();
    let result = queue.dequeue().await?;
    let elapsed = start.elapsed();

    assert!(result.is_none(), "Should return None on timeout");
    assert!(
        elapsed >= Duration::from_secs(2) && elapsed < Duration::from_secs(3),
        "Should timeout after approximately 2 seconds, got {:?}",
        elapsed
    );

    Ok(())
}

#[tokio::test]
async fn test_concurrent_workers() -> Result<()> {
    common::init_test_logging();

    // Start Redis container
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    // Create multiple jobs
    let mut queue = ReliableQueue::new(&redis_url, "test_concurrent_queue", 1).await?;

    let job_count = 10;
    for i in 0..job_count {
        let job = Job {
            id: format!("concurrent-job-{}", i),
            repo_url: "git@github.com:test/repo.git".to_string(),
            branch: "main".to_string(),
            prompt: format!("Task {}", i),
            mcp_connection_url: None,
        };
        queue.enqueue(&job).await?;
    }

    // Spawn multiple workers to process jobs concurrently
    let mut handles = vec![];
    for worker_id in 0..3 {
        let redis_url = redis_url.clone();
        let handle = tokio::spawn(async move {
            let mut worker_queue = ReliableQueue::new(&redis_url, "test_concurrent_queue", 1)
                .await
                .unwrap();

            let mut processed = 0;
            while let Some(job) = worker_queue.dequeue().await.unwrap() {
                // Simulate work
                tokio::time::sleep(Duration::from_millis(50)).await;

                // ACK the job
                worker_queue.ack(&job).await.unwrap();
                processed += 1;

                // Stop after processing a few jobs (to avoid infinite loop)
                if processed >= 5 {
                    break;
                }
            }

            (worker_id, processed)
        });
        handles.push(handle);
    }

    // Wait for all workers
    let mut total_processed = 0;
    for handle in handles {
        let (worker_id, processed) = handle.await?;
        println!("Worker {} processed {} jobs", worker_id, processed);
        total_processed += processed;
    }

    // Verify all jobs were processed (or at least attempted)
    assert!(
        total_processed >= job_count,
        "All jobs should be processed, got {}",
        total_processed
    );

    Ok(())
}
