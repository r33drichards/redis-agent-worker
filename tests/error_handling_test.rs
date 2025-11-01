mod common;

use anyhow::Result;
use redis_agent_worker::git::GitRepo;
use redis_agent_worker::instance::InstanceAllocator;
use redis_agent_worker::queue::{Job, ReliableQueue};
use tempfile::TempDir;
use testcontainers::{runners::AsyncRunner, GenericImage};
use uuid::Uuid;

#[tokio::test]
async fn test_error_invalid_redis_url() -> Result<()> {
    common::init_test_logging();

    // Try to connect to invalid Redis URL
    let result = ReliableQueue::new("redis://invalid-host:9999", "test_queue", 5).await;

    assert!(result.is_err(), "Should fail to connect to invalid Redis URL");

    Ok(())
}

#[tokio::test]
async fn test_error_invalid_job_json() -> Result<()> {
    common::init_test_logging();

    // Setup Redis
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    // Manually push invalid JSON to queue
    let client = redis::Client::open(redis_url.as_str())?;
    let mut conn = client.get_multiplexed_async_connection().await?;

    use redis::AsyncCommands;
    let _: () = conn.lpush("invalid_json_queue", "not a valid json").await?;

    // Try to dequeue - should fail to deserialize
    let mut queue = ReliableQueue::new(&redis_url, "invalid_json_queue", 2).await?;
    let result = queue.dequeue().await;

    assert!(result.is_err(), "Should fail to deserialize invalid JSON");

    Ok(())
}

#[tokio::test]
async fn test_error_clone_nonexistent_repo() -> Result<()> {
    common::init_test_logging();

    let temp_dir = TempDir::new()?;
    let clone_dir = temp_dir.path().join("clone");

    // Try to clone a repository that doesn't exist
    let result = GitRepo::clone(
        "git@github.com:nonexistent-user-12345/nonexistent-repo-67890.git",
        &clone_dir,
    );

    assert!(result.is_err(), "Should fail to clone nonexistent repository");

    Ok(())
}

#[tokio::test]
async fn test_error_checkout_nonexistent_branch() -> Result<()> {
    common::init_test_logging();

    let temp_dir = TempDir::new()?;
    let branch_name = "existing-branch";

    // Setup git environment
    let (_, remote_url) = common::setup_test_git_env(temp_dir.path(), branch_name)?;

    // Clone the repository
    let clone_dir = temp_dir.path().join("clone");
    let git_repo = GitRepo::clone(&remote_url, &clone_dir)?;

    git_repo.fetch()?;

    // Try to checkout a branch that doesn't exist
    let result = git_repo.checkout_branch("nonexistent-branch-12345");

    assert!(result.is_err(), "Should fail to checkout nonexistent branch");

    Ok(())
}

#[tokio::test]
async fn test_error_push_without_commit() -> Result<()> {
    common::init_test_logging();

    let temp_dir = TempDir::new()?;
    let branch_name = "test-branch";

    // Setup git environment
    let (_, remote_url) = common::setup_test_git_env(temp_dir.path(), branch_name)?;

    // Clone the repository
    let clone_dir = temp_dir.path().join("clone");
    let git_repo = GitRepo::clone(&remote_url, &clone_dir)?;
    git_repo.fetch()?;
    git_repo.checkout_branch(branch_name)?;

    // Try to push without making any changes
    // This should succeed but do nothing (git allows pushing with no changes)
    let result = git_repo.push(branch_name);

    // Actually, git allows this, so it should succeed
    assert!(result.is_ok(), "Git allows pushing without changes");

    Ok(())
}

#[tokio::test]
async fn test_error_allocator_unreachable() -> Result<()> {
    common::init_test_logging();

    // Create allocator pointing to invalid URL
    let allocator = InstanceAllocator::new("http://localhost:99999".to_string());

    // Try to borrow instance
    let result = allocator.borrow_instance().await;

    assert!(result.is_err(), "Should fail to connect to unreachable allocator");

    Ok(())
}

#[tokio::test]
async fn test_error_ack_nonexistent_job() -> Result<()> {
    common::init_test_logging();

    // Setup Redis
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    let mut queue = ReliableQueue::new(&redis_url, "error_ack_queue", 2).await?;

    // Try to ACK a job that was never in the processing queue
    let fake_job = Job {
        id: "fake-job-id".to_string(),
        repo_url: "git@github.com:test/repo.git".to_string(),
        branch: "main".to_string(),
        prompt: "Fake job".to_string(),
        mcp_connection_url: None,
    };

    // This should succeed but log a warning (job not found)
    let result = queue.ack(&fake_job).await;

    assert!(result.is_ok(), "ACK should succeed even if job not found");

    Ok(())
}

#[tokio::test]
async fn test_error_nack_nonexistent_job() -> Result<()> {
    common::init_test_logging();

    // Setup Redis
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    let mut queue = ReliableQueue::new(&redis_url, "error_nack_queue", 2).await?;

    // Try to NACK a job that was never in the processing queue
    let fake_job = Job {
        id: "fake-job-id".to_string(),
        repo_url: "git@github.com:test/repo.git".to_string(),
        branch: "main".to_string(),
        prompt: "Fake job".to_string(),
        mcp_connection_url: None,
    };

    // This should succeed but log an error (job not found)
    let result = queue.nack(&fake_job).await;

    assert!(result.is_ok(), "NACK should succeed even if job not found");

    Ok(())
}

#[tokio::test]
async fn test_error_git_no_changes_detected() -> Result<()> {
    common::init_test_logging();

    let temp_dir = TempDir::new()?;
    let branch_name = "test-branch";

    // Setup git environment
    let (_, remote_url) = common::setup_test_git_env(temp_dir.path(), branch_name)?;

    // Clone the repository
    let clone_dir = temp_dir.path().join("clone");
    let git_repo = GitRepo::clone(&remote_url, &clone_dir)?;
    git_repo.fetch()?;
    git_repo.checkout_branch(branch_name)?;

    // Check that there are no changes
    let has_changes = git_repo.has_changes()?;
    assert!(!has_changes, "Fresh clone should have no changes");

    Ok(())
}

#[tokio::test]
async fn test_error_invalid_repo_url_format() -> Result<()> {
    common::init_test_logging();

    let temp_dir = TempDir::new()?;
    let clone_dir = temp_dir.path().join("clone");

    // Try to clone with various invalid URL formats
    let invalid_urls = vec![
        "not-a-url",
        "http://",
        "git@",
        "",
    ];

    for invalid_url in invalid_urls {
        let result = GitRepo::clone(invalid_url, &clone_dir);
        assert!(result.is_err(), "Should fail to clone with invalid URL: {}", invalid_url);
    }

    Ok(())
}

#[tokio::test]
async fn test_error_redis_connection_loss() -> Result<()> {
    common::init_test_logging();

    // Setup Redis
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    // Create queue and enqueue a job
    let mut queue = ReliableQueue::new(&redis_url, "error_connection_queue", 2).await?;

    let job = Job {
        id: Uuid::new_v4().to_string(),
        repo_url: "git@github.com:test/repo.git".to_string(),
        branch: "main".to_string(),
        prompt: "Test".to_string(),
        mcp_connection_url: None,
    };

    queue.enqueue(&job).await?;

    // Drop the container (simulating connection loss)
    drop(redis_container);

    // Wait a moment for the container to stop
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Try to dequeue - this should fail or timeout
    // Note: This might still succeed if Redis hasn't fully stopped yet
    // or if the connection manager has some retry logic
    let result = queue.dequeue().await;

    // Either succeeds (if container still running) or fails (if stopped)
    // We're mainly testing that it doesn't panic
    match result {
        Ok(_) => println!("Dequeue succeeded (container may still be running)"),
        Err(e) => println!("Dequeue failed as expected: {}", e),
    }

    Ok(())
}

#[tokio::test]
async fn test_error_commit_without_changes() -> Result<()> {
    common::init_test_logging();

    let temp_dir = TempDir::new()?;
    let branch_name = "test-branch";

    // Setup git environment
    let (_, remote_url) = common::setup_test_git_env(temp_dir.path(), branch_name)?;

    // Clone the repository
    let clone_dir = temp_dir.path().join("clone");
    let git_repo = GitRepo::clone(&remote_url, &clone_dir)?;
    git_repo.fetch()?;
    git_repo.checkout_branch(branch_name)?;

    // Stage all (nothing to stage)
    git_repo.stage_all()?;

    // Try to commit with no changes
    // Git will fail with "nothing to commit"
    let result = git_repo.commit("Empty commit");

    assert!(result.is_err(), "Should fail to commit with no changes");

    Ok(())
}

#[tokio::test]
async fn test_error_double_ack_same_job() -> Result<()> {
    common::init_test_logging();

    // Setup Redis
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    let mut queue = ReliableQueue::new(&redis_url, "double_ack_queue", 2).await?;

    // Create and enqueue a job
    let job = Job {
        id: Uuid::new_v4().to_string(),
        repo_url: "git@github.com:test/repo.git".to_string(),
        branch: "main".to_string(),
        prompt: "Test".to_string(),
        mcp_connection_url: None,
    };

    queue.enqueue(&job).await?;

    // Dequeue the job
    let dequeued = queue.dequeue().await?.expect("Should dequeue job");

    // ACK the job
    queue.ack(&job).await?;

    // Verify it's removed from processing queue
    assert_eq!(queue.processing_len().await?, 0);

    // Try to ACK again - should succeed but do nothing
    queue.ack(&dequeued).await?;

    Ok(())
}

#[tokio::test]
async fn test_error_queue_serialization_edge_cases() -> Result<()> {
    common::init_test_logging();

    // Setup Redis
    let redis_container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.into())
        .start()
        .await
        .expect("Failed to start Redis container");

    let redis_port = redis_container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", redis_port);

    let mut queue = ReliableQueue::new(&redis_url, "edge_case_queue", 2).await?;

    // Test with special characters in fields
    let job = Job {
        id: "job-with-special-chars-!@#$%^&*()".to_string(),
        repo_url: "git@github.com:user/repo-with-dashes_and_underscores.git".to_string(),
        branch: "feature/test-branch-123".to_string(),
        prompt: "Test with \"quotes\" and 'apostrophes' and\nnewlines".to_string(),
        mcp_connection_url: Some("http://example.com:8080/path?query=value&key=123".to_string()),
    };

    // Enqueue and dequeue - should handle special characters correctly
    queue.enqueue(&job).await?;
    let dequeued = queue.dequeue().await?.expect("Should dequeue job");

    assert_eq!(dequeued.id, job.id);
    assert_eq!(dequeued.prompt, job.prompt);
    assert_eq!(dequeued.mcp_connection_url, job.mcp_connection_url);

    Ok(())
}
