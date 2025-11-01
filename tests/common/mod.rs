use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockInstance {
    pub id: String,
    pub mcp_connection_url: String,
    pub api_url: String,
}

#[derive(Debug, Clone)]
pub struct MockAllocatorState {
    pub borrowed_instances: Arc<Mutex<Vec<MockInstance>>>,
    pub returned_instances: Arc<Mutex<Vec<MockInstance>>>,
    pub next_instance_id: Arc<Mutex<u32>>,
}

impl MockAllocatorState {
    pub fn new() -> Self {
        Self {
            borrowed_instances: Arc::new(Mutex::new(Vec::new())),
            returned_instances: Arc::new(Mutex::new(Vec::new())),
            next_instance_id: Arc::new(Mutex::new(1)),
        }
    }

    pub async fn borrow_count(&self) -> usize {
        self.borrowed_instances.lock().await.len()
    }

    pub async fn return_count(&self) -> usize {
        self.returned_instances.lock().await.len()
    }
}

async fn borrow_handler(
    State(state): State<MockAllocatorState>,
) -> Result<Json<MockInstance>, StatusCode> {
    let mut id_counter = state.next_instance_id.lock().await;
    let instance_id = *id_counter;
    *id_counter += 1;
    drop(id_counter);

    let instance = MockInstance {
        id: format!("mock-instance-{}", instance_id),
        mcp_connection_url: format!("http://mock-mcp-{}.example.com", instance_id),
        api_url: format!("http://mock-api-{}.example.com", instance_id),
    };

    info!("Mock allocator: Borrowing instance {}", instance.id);
    state.borrowed_instances.lock().await.push(instance.clone());

    Ok(Json(instance))
}

async fn return_handler(
    State(state): State<MockAllocatorState>,
    Json(instance): Json<MockInstance>,
) -> StatusCode {
    info!("Mock allocator: Returning instance {}", instance.id);
    state.returned_instances.lock().await.push(instance);
    StatusCode::OK
}

async fn health_handler() -> &'static str {
    "OK"
}

pub async fn start_mock_allocator() -> (String, MockAllocatorState) {
    let state = MockAllocatorState::new();

    let app = Router::new()
        .route("/borrow", post(borrow_handler))
        .route("/return", post(return_handler))
        .route("/health", get(health_handler))
        .with_state(state.clone());

    let addr = SocketAddr::from(([127, 0, 0, 1], 0)); // Bind to random port
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let bound_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let url = format!("http://{}", bound_addr);
    info!("Mock allocator started at {}", url);

    (url, state)
}

/// Create a mock git repository for testing
pub fn create_mock_git_repo(path: &std::path::Path, branch: &str) -> anyhow::Result<()> {
    use git2::{Repository, Signature};

    // Initialize repository
    let repo = Repository::init(path)?;

    // Create initial commit on main branch
    let mut index = repo.index()?;

    // Create a test file
    let test_file = path.join("README.md");
    std::fs::write(&test_file, "# Test Repository\n")?;

    index.add_path(std::path::Path::new("README.md"))?;
    index.write()?;

    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let sig = Signature::now("Test User", "test@example.com")?;

    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;

    // Create the test branch if it's not main
    if branch != "main" && branch != "master" {
        let head = repo.head()?;
        let commit = head.peel_to_commit()?;
        repo.branch(branch, &commit, false)?;
    }

    info!("Created mock git repository at {:?} with branch {}", path, branch);

    Ok(())
}

/// Create a bare git repository that can be used as a remote
pub fn create_bare_git_repo(path: &std::path::Path) -> anyhow::Result<()> {
    use git2::Repository;

    Repository::init_bare(path)?;
    info!("Created bare git repository at {:?}", path);

    Ok(())
}

/// Setup a complete test git environment with local and remote repos
pub fn setup_test_git_env(
    work_dir: &std::path::Path,
    branch: &str,
) -> anyhow::Result<(std::path::PathBuf, String)> {
    // Create bare "remote" repository
    let remote_path = work_dir.join("remote.git");
    create_bare_git_repo(&remote_path)?;

    // Create local repository
    let local_path = work_dir.join("local");
    std::fs::create_dir_all(&local_path)?;

    // Initialize and setup local repo
    use git2::{Repository, Signature};
    let repo = Repository::init(&local_path)?;

    // Add remote
    let remote_url = format!("file://{}", remote_path.display());
    repo.remote("origin", &remote_url)?;

    // Create initial commit
    let mut index = repo.index()?;
    let readme_path = local_path.join("README.md");
    std::fs::write(&readme_path, "# Test Project\n\nThis is a test repository.\n")?;

    index.add_path(std::path::Path::new("README.md"))?;
    index.write()?;

    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let sig = Signature::now("Test Bot", "bot@test.com")?;

    let commit_oid = repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;

    // Push to remote
    let mut remote = repo.find_remote("origin")?;
    remote.push(&["refs/heads/main:refs/heads/main"], None)?;

    // Create and push the test branch
    if branch != "main" {
        let commit = repo.find_commit(commit_oid)?;
        repo.branch(branch, &commit, false)?;

        // Checkout the branch
        let obj = repo.revparse_single(&format!("refs/heads/{}", branch))?;
        repo.checkout_tree(&obj, None)?;
        repo.set_head(&format!("refs/heads/{}", branch))?;

        // Push the branch
        remote.push(&[format!("refs/heads/{}:refs/heads/{}", branch, branch)], None)?;
    }

    info!("Setup test git environment: remote at {:?}, local at {:?}", remote_path, local_path);

    Ok((local_path, remote_url))
}

/// Initialize test logging
pub fn init_test_logging() {
    use tracing_subscriber::EnvFilter;

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,redis_agent_worker=debug"))
        )
        .with_test_writer()
        .try_init();
}
