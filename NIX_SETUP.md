# Nix Flake Setup Guide

This repository now uses Nix flakes for reproducible development environments and builds.

## What Was Added

### 1. `flake.nix`
The main Nix flake configuration that provides:
- **Development shells**: Full development environment with all dependencies
- **CI shell**: Minimal environment for CI/CD
- **Build configuration**: Nix-based build for the Rust project
- **Apps**: Direct execution with `nix run`

### 2. `flake.lock`
Lock file that pins all Nix dependencies to specific versions for reproducibility.

### 3. `.envrc`
Direnv configuration for automatic environment loading when entering the directory.

### 4. `Cargo.lock`
Rust dependency lock file (now tracked in git as this is an application).

### 5. Updated `.gitignore`
Added Nix-specific ignores:
- `.direnv/` - direnv cache
- `result` - Nix build symlinks
- `result-*` - Additional Nix build artifacts

## Quick Start

### Prerequisites
- Nix with flakes enabled
- Optionally: direnv for automatic environment loading

### Enable Nix Flakes
If you haven't already, enable flakes in your Nix configuration:

```bash
# Add to ~/.config/nix/nix.conf or /etc/nix/nix.conf
experimental-features = nix-command flakes
```

## Usage

### Development Shell

Enter the full development environment:

```bash
nix develop
```

This provides:
- Rust toolchain (cargo, rustc, clippy, rust-analyzer)
- Docker and docker-compose
- Git
- Redis client
- All necessary build dependencies
- Development tools (cargo-watch, cargo-edit, cargo-audit)

### Automatic Environment Loading (Recommended)

Install direnv and allow the environment:

```bash
# Install direnv (if not already installed)
nix-env -iA nixpkgs.direnv

# Enable direnv
echo 'eval "$(direnv hook bash)"' >> ~/.bashrc  # or ~/.zshrc for zsh
source ~/.bashrc

# Allow the environment
direnv allow
```

Now, when you `cd` into the project directory, the environment will be automatically loaded!

### Building

#### With Nix
```bash
# Build the project (creates ./result symlink)
nix build

# Run the binary
./result/bin/redis-agent-worker --help

# Or run directly without building
nix run . -- --help
```

#### Traditional Cargo (in nix shell)
```bash
nix develop
cargo build --release
```

### Running Tests

Tests require Docker for testcontainers. Make sure Docker daemon is running:

```bash
# Enter development shell
nix develop

# Run all tests
cargo test

# Or use the test script
./run_tests.sh all

# Run specific test suites
./run_tests.sh integration  # Integration tests only
./run_tests.sh verbose      # With debug output
./run_tests.sh ci           # Full CI suite
```

### CI Environment

For CI/CD, use the minimal CI shell:

```bash
nix develop .#ci
```

This includes only essential dependencies (Rust, Docker, Git) for faster builds.

## Available Tools in Development Shell

The development shell includes:

- **Rust Toolchain**:
  - `cargo` - Rust package manager
  - `rustc` - Rust compiler
  - `clippy` - Rust linter
  - `rust-analyzer` - LSP server for IDEs
  - `cargo-watch` - Auto-rebuild on file changes
  - `cargo-edit` - `cargo add`, `cargo rm` commands
  - `cargo-audit` - Security audit tool

- **Container Tools**:
  - `docker` - Container runtime
  - `docker-compose` - Multi-container orchestration
  - `redis` - Redis client

- **Development Utilities**:
  - `git` - Version control
  - `jq` - JSON processor
  - `curl` - HTTP client
  - `netcat` - Network utility

## Test Infrastructure

### What Was Added

1. **Integration Tests** (`tests/integration_test.rs`) - Already existed, covers:
   - Queue operations (enqueue/dequeue/ack/nack)
   - Queue recovery
   - Instance allocator operations
   - Git operations
   - Full workflow simulation
   - Concurrent workers
   - Blocking queue timeout

2. **E2E Tests** (`tests/e2e_test.rs`) - NEW:
   - Worker statistics
   - Worker recovery on startup
   - Job failure and retry logic
   - Multiple jobs sequential processing
   - Instance guard cleanup
   - Empty queue timeout behavior
   - Git merge conflict scenarios
   - Jobs with MCP connection
   - Concurrent queue operations

3. **Error Handling Tests** (`tests/error_handling_test.rs`) - NEW:
   - Invalid Redis URL
   - Invalid JSON in queue
   - Nonexistent repository cloning
   - Nonexistent branch checkout
   - Push without commit
   - Unreachable allocator
   - ACK/NACK nonexistent jobs
   - No changes detected
   - Invalid repository URL formats
   - Redis connection loss
   - Commit without changes
   - Double ACK same job
   - Serialization edge cases

4. **Common Test Utilities** (`tests/common/mod.rs`):
   - Mock allocator server (using Axum)
   - Git repository setup helpers
   - Test logging initialization

### Running Tests

All tests use testcontainers to spin up Redis automatically, so you need Docker running:

```bash
# Inside nix develop shell

# Run all tests
make test

# Run integration tests only
make test-integration

# Run with verbose output
make test-verbose

# Run with debug logging
RUST_LOG=debug cargo test -- --nocapture

# Run specific test
cargo test test_queue_enqueue_dequeue

# Using the test script
./run_tests.sh all          # All tests
./run_tests.sh integration  # Integration only
./run_tests.sh verbose      # With full output
./run_tests.sh ci           # CI suite (checks + tests)
```

## Troubleshooting

### Docker Issues

If tests fail due to Docker:

1. **Check Docker daemon is running**:
   ```bash
   docker ps
   ```

2. **On NixOS**, ensure Docker service is enabled:
   ```nix
   virtualisation.docker.enable = true;
   ```

3. **Check Docker permissions**:
   ```bash
   # Add user to docker group
   sudo usermod -aG docker $USER
   # Log out and back in
   ```

### Nix Build Issues

If `nix build` fails:

1. **Clear the cache**:
   ```bash
   nix-collect-garbage
   ```

2. **Update flake inputs**:
   ```bash
   nix flake update
   ```

3. **Rebuild from scratch**:
   ```bash
   nix build --rebuild
   ```

### Direnv Not Working

If the environment doesn't load automatically:

1. **Check direnv is installed and hooked**:
   ```bash
   direnv version
   echo $DIRENV_DIR  # Should be non-empty in project dir
   ```

2. **Re-allow the directory**:
   ```bash
   direnv allow
   ```

## Benefits of This Setup

1. **Reproducibility**: Same environment on every machine
2. **No Global Installation**: Everything is project-scoped
3. **Version Pinning**: Exact versions locked in `flake.lock`
4. **Easy Onboarding**: New developers just need `nix develop`
5. **CI/CD Ready**: Same environment locally and in CI
6. **Automatic Cleanup**: Dependencies removed with `nix-collect-garbage`

## Next Steps

1. Try entering the development shell: `nix develop`
2. Run the tests: `cargo test`
3. Build the project: `nix build`
4. Set up direnv for automatic loading: `direnv allow`

## Additional Resources

- [Nix Manual](https://nixos.org/manual/nix/stable/)
- [Nix Flakes](https://nixos.wiki/wiki/Flakes)
- [direnv Documentation](https://direnv.net/)
- [Rust Overlay](https://github.com/oxalica/rust-overlay)
