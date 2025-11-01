use anyhow::{Context, Result};
use git2::{
    BranchType, Cred, Direction, FetchOptions, RemoteCallbacks, Repository,
};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

pub struct GitRepo {
    repo: Repository,
    repo_path: PathBuf,
}

impl GitRepo {
    /// Clone a repository to a temporary directory
    pub fn clone(repo_url: &str, target_dir: &Path) -> Result<Self> {
        info!("Cloning repository: {} to {:?}", repo_url, target_dir);

        // Setup callbacks for authentication
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, _allowed_types| {
            debug!("Git credentials callback");
            Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"))
        });

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);

        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fetch_options);

        let repo = builder
            .clone(repo_url, target_dir)
            .context("Failed to clone repository")?;

        info!("Successfully cloned repository to {:?}", target_dir);

        Ok(Self {
            repo,
            repo_path: target_dir.to_path_buf(),
        })
    }

    /// Open an existing repository
    pub fn open(repo_path: &Path) -> Result<Self> {
        let repo = Repository::open(repo_path)
            .context("Failed to open repository")?;

        Ok(Self {
            repo,
            repo_path: repo_path.to_path_buf(),
        })
    }

    /// Checkout a specific branch
    pub fn checkout_branch(&self, branch_name: &str) -> Result<()> {
        info!("Checking out branch: {}", branch_name);

        // First, try to find the branch locally
        let branch = self.repo.find_branch(branch_name, BranchType::Local);

        let (object, reference) = match branch {
            Ok(branch) => {
                debug!("Found local branch: {}", branch_name);
                let reference = branch.get().name().context("Invalid branch name")?;
                let object = self.repo.revparse_single(reference)?;
                (object, reference.to_string())
            }
            Err(_) => {
                // Branch doesn't exist locally, try remote
                debug!("Branch not found locally, checking remote");

                let remote_branch = format!("origin/{}", branch_name);
                let object = self.repo.revparse_single(&remote_branch)
                    .context("Failed to find branch in remote")?;

                // Create local branch tracking remote
                let commit = object.peel_to_commit()?;
                self.repo.branch(branch_name, &commit, false)
                    .context("Failed to create local branch")?;

                let reference = format!("refs/heads/{}", branch_name);
                (object, reference)
            }
        };

        // Checkout the branch
        self.repo.checkout_tree(&object, None)?;
        self.repo.set_head(&reference)?;

        info!("Successfully checked out branch: {}", branch_name);
        Ok(())
    }

    /// Stage all changes
    pub fn stage_all(&self) -> Result<()> {
        info!("Staging all changes");

        let mut index = self.repo.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;

        debug!("Successfully staged all changes");
        Ok(())
    }

    /// Commit changes
    pub fn commit(&self, message: &str) -> Result<()> {
        info!("Creating commit with message: {}", message);

        let mut index = self.repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;

        let signature = self.repo.signature()?;
        let parent_commit = self.repo.head()?.peel_to_commit()?;

        self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent_commit],
        )?;

        info!("Successfully created commit");
        Ok(())
    }

    /// Push changes to remote
    pub fn push(&self, branch_name: &str) -> Result<()> {
        info!("Pushing branch: {}", branch_name);

        let mut remote = self.repo.find_remote("origin")
            .context("Failed to find origin remote")?;

        // Setup callbacks for authentication
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, _allowed_types| {
            debug!("Git credentials callback for push");
            Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"))
        });

        let mut push_options = git2::PushOptions::new();
        push_options.remote_callbacks(callbacks);

        let refspec = format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name);

        remote.push(&[&refspec], Some(&mut push_options))
            .context("Failed to push changes")?;

        info!("Successfully pushed branch: {}", branch_name);
        Ok(())
    }

    /// Fetch from remote
    pub fn fetch(&self) -> Result<()> {
        info!("Fetching from remote");

        let mut remote = self.repo.find_remote("origin")?;

        // Setup callbacks for authentication
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, _allowed_types| {
            Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"))
        });

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);

        remote.fetch(&["refs/heads/*:refs/remotes/origin/*"], Some(&mut fetch_options), None)?;

        info!("Successfully fetched from remote");
        Ok(())
    }

    /// Get the repository path
    pub fn path(&self) -> &Path {
        &self.repo_path
    }

    /// Check if there are uncommitted changes
    pub fn has_changes(&self) -> Result<bool> {
        let statuses = self.repo.statuses(None)?;
        Ok(!statuses.is_empty())
    }
}
