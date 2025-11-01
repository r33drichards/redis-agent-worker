# Integration Tests

This directory contains end-to-end integration tests for the redis-agent-worker.

## Overview

The tests use real dependencies where possible:
- **Redis**: Real Redis instance via testcontainers (Docker)
- **Instance Allocator**: Mock HTTP server using Axum
- **Git Operations**: Real git operations with temporary repositories
- **Agent Execution**: Mocked (since Hyperlight is not available in test environment)

## Test Coverage

### Queue Operations (`test_queue_*`)

1. **test_queue_enqueue_dequeue**: Tests basic enqueue/dequeue operations and reliable queue pattern (RPOPLPUSH)
2. **test_queue_nack_retry**: Tests job retry mechanism when processing fails
3. **test_queue_recovery**: Tests recovery of stalled jobs after worker crash
4. **test_blocking_queue_timeout**: Tests blocking dequeue with timeout

### Instance Allocator (`test_instance_allocator`)

Tests the instance borrowing and returning mechanism with a mock HTTP allocator service.

### Git Operations (`test_git_operations`)

Tests the complete git workflow:
- Cloning repositories
- Checking out branches
- Making changes
- Committing and pushing

### Full Workflow (`test_full_workflow_with_mock_agent`)

End-to-end test that simulates the complete worker flow:
1. Enqueue job to Redis
2. Worker dequeues job
3. Borrow instance from allocator
4. Clone git repository
5. Checkout branch
6. Make changes (simulated agent work)
7. Commit and push changes
8. Return instance
9. ACK job completion
10. Verify changes were pushed

### Concurrent Workers (`test_concurrent_workers`)

Tests multiple workers processing jobs concurrently from the same queue.

## Prerequisites

- Docker (for testcontainers)
- Internet connection (to pull Redis image)
- Git installed locally

## Running Tests

### Run all tests

```bash
cargo test
```

### Run integration tests only

```bash
cargo test --test integration_test
```

### Run specific test

```bash
cargo test test_queue_enqueue_dequeue
```

### Run with output

```bash
cargo test -- --nocapture
```

### Run with detailed logging

```bash
RUST_LOG=debug cargo test -- --nocapture
```

## Test Structure

```
tests/
├── common/
│   └── mod.rs          # Shared test utilities
│       ├── MockAllocatorState
│       ├── start_mock_allocator()
│       ├── setup_test_git_env()
│       └── init_test_logging()
└── integration_test.rs  # Main test suite
```

## Common Utilities

### Mock Allocator

The mock allocator implements the same API as the real instance allocator:
- `POST /borrow` - Returns a mock instance
- `POST /return` - Accepts instance return
- `GET /health` - Health check

### Git Test Environment

`setup_test_git_env()` creates:
- A bare git repository (simulating remote)
- A local git repository with initial commit
- A test branch
- Proper git remote configuration

### Test Logging

`init_test_logging()` initializes tracing for test output:
- Logs to test output
- Configurable via RUST_LOG environment variable
- Default level: info, debug for redis_agent_worker

## Troubleshooting

### Docker Not Running

If you see errors about Docker not being available:
```
Error: Failed to connect to Docker daemon
```

Make sure Docker is running:
```bash
docker ps
```

### Port Conflicts

If tests fail with port binding errors, make sure no other services are using conflicting ports. The mock allocator uses a random port, but Redis container uses Docker port mapping which should handle conflicts.

### Git SSH Keys

The tests use `file://` URLs for git operations to avoid SSH key requirements. If you modify tests to use SSH URLs, ensure your SSH keys are properly configured.

## CI/CD Integration

These tests are designed to run in CI/CD environments:

```yaml
# Example GitHub Actions
- name: Run integration tests
  run: cargo test --test integration_test
  env:
    RUST_LOG: info
```

Make sure your CI environment has Docker available.

## Performance

Test execution time:
- Individual queue tests: ~2-3 seconds (includes Redis startup)
- Git operations: ~1-2 seconds
- Full workflow test: ~3-5 seconds
- Concurrent workers: ~2-3 seconds

Total suite: ~20-30 seconds

## Known Limitations

1. **Agent Execution**: The agent (Hyperlight) is not actually executed; changes are simulated
2. **Network Policies**: Hyperlight network restrictions are not tested
3. **SSH Authentication**: Tests use file:// URLs instead of git@github.com URLs
4. **Instance Cleanup**: In case of test failures, Docker containers should be cleaned up automatically

## Future Improvements

- [ ] Add stress tests with many jobs
- [ ] Test network failure scenarios
- [ ] Add metrics and monitoring tests
- [ ] Test different error conditions
- [ ] Add benchmarks for queue operations
- [ ] Test with actual Hyperlight if available in CI
