# Redis Agent Worker

A reliable Redis-based worker system for processing agent jobs with automatic instance management, git operations, and sandboxed execution using Hyperlight.

## Features

- **Reliable Queue Pattern**: Uses Redis RPOPLPUSH for guaranteed job processing
- **Automatic Instance Management**: Borrows and returns compute instances from an allocator service
- **Git Integration**: Clones repositories, checks out branches, and pushes changes
- **Sandboxed Execution**: Runs agents in Hyperlight with restricted network permissions (MCP-only access)
- **Automatic Recovery**: Recovers stalled jobs on startup
- **RAII Instance Management**: Ensures instances are returned even on panic

## Architecture

```
┌─────────────┐
│ Redis Queue │
└──────┬──────┘
       │
       ▼
┌──────────────────────┐
│  Worker Process      │
│  ┌────────────────┐  │
│  │ 1. Dequeue Job │  │
│  └────────┬───────┘  │
│           ▼          │
│  ┌────────────────┐  │
│  │ 2. Borrow      │  │
│  │    Instance    │  │
│  └────────┬───────┘  │
│           ▼          │
│  ┌────────────────┐  │
│  │ 3. Clone Repo  │  │
│  └────────┬───────┘  │
│           ▼          │
│  ┌────────────────┐  │
│  │ 4. Checkout    │  │
│  │    Branch      │  │
│  └────────┬───────┘  │
│           ▼          │
│  ┌────────────────┐  │
│  │ 5. Run Agent   │  │
│  │    (Hyperlight)│  │
│  └────────┬───────┘  │
│           ▼          │
│  ┌────────────────┐  │
│  │ 6. Commit &    │  │
│  │    Push        │  │
│  └────────┬───────┘  │
│           ▼          │
│  ┌────────────────┐  │
│  │ 7. Return      │  │
│  │    Instance    │  │
│  └────────┬───────┘  │
│           ▼          │
│  ┌────────────────┐  │
│  │ 8. ACK Job     │  │
│  └────────────────┘  │
└──────────────────────┘
```

## Prerequisites

- Rust 1.70+
- Redis server
- Git with SSH key authentication configured
- Instance allocator service (compatible with `ip-allocator-webserver`)
- Hyperlight runtime

## Installation

```bash
cargo build --release
```

The binary will be available at `target/release/redis-agent-worker`.

## Configuration

Configuration can be provided via command-line arguments or environment variables:

| Environment Variable  | CLI Flag                | Default                    | Description                           |
|-----------------------|-------------------------|----------------------------|---------------------------------------|
| `REDIS_URL`           | `--redis-url`           | `redis://127.0.0.1:6379`   | Redis connection URL                  |
| `QUEUE_NAME`          | `--queue-name`          | `agent_jobs`               | Name of the Redis queue               |
| `ALLOCATOR_API_URL`   | `--allocator-api-url`   | `http://localhost:8080`    | Instance allocator API endpoint       |
| `HYPERLIGHT_PATH`     | `--hyperlight-path`     | `/usr/local/bin/hyperlight`| Path to Hyperlight executable         |
| `WORK_DIR`            | `--work-dir`            | `/tmp/agent-worker`        | Working directory for repositories    |
| `LOG_LEVEL`           | `--log-level`           | `info`                     | Log level (trace/debug/info/warn/error)|

### Example .env file

```bash
REDIS_URL=redis://localhost:6379
QUEUE_NAME=agent_jobs
ALLOCATOR_API_URL=http://localhost:8080
HYPERLIGHT_PATH=/usr/local/bin/hyperlight
WORK_DIR=/tmp/agent-worker
LOG_LEVEL=info
```

## Usage

### Run the Worker

Start the worker to process jobs from the queue:

```bash
redis-agent-worker run --timeout 30
```

### Enqueue a Job

Add a new job to the queue:

```bash
redis-agent-worker enqueue \
  --job-id "job-123" \
  --repo-url "git@github.com:user/repo.git" \
  --branch "feature-branch" \
  --prompt "Implement a new feature X" \
  --mcp-connection-url "http://mcp.example.com"
```

### View Queue Statistics

Check the current queue status:

```bash
redis-agent-worker stats
```

### Peek at Next Job

View the next job without dequeuing:

```bash
redis-agent-worker peek
```

### Recover Stalled Jobs

Manually recover jobs that were being processed when a worker crashed:

```bash
redis-agent-worker recover
```

## Job Format

Jobs are JSON objects with the following structure:

```json
{
  "id": "unique-job-id",
  "repo_url": "git@github.com:user/repo.git",
  "branch": "feature-branch",
  "prompt": "The task for the agent to perform",
  "mcp_connection_url": "http://mcp.example.com" // optional
}
```

## Instance Allocator API

The worker expects an instance allocator service with the following endpoints:

### POST /borrow

Request a new instance.

**Response:**
```json
{
  "id": "instance-123",
  "mcp_connection_url": "http://mcp.example.com",
  "api_url": "http://api.example.com"
}
```

### POST /return

Return an instance.

**Request Body:**
```json
{
  "id": "instance-123",
  "mcp_connection_url": "http://mcp.example.com",
  "api_url": "http://api.example.com"
}
```

## Hyperlight Integration

The agent is executed using Hyperlight with the following environment variables set:

- `MCP_CONNECTION_URL`: The MCP server URL for agent communication
- `HYPERLIGHT_ALLOW_NETWORK`: Set to `"mcp_only"` or `"false"`
- `HYPERLIGHT_MCP_URL`: Allowed MCP connection URL
- `HYPERLIGHT_WORKING_DIR`: Repository path
- `HYPERLIGHT_ALLOW_FILE_WRITE`: Set to `"true"`
- `HYPERLIGHT_ALLOW_FILE_READ`: Set to `"true"`

## Reliable Queue Pattern

The worker implements the reliable queue pattern using Redis:

1. **Dequeue**: Uses `BRPOPLPUSH` to atomically move job from main queue to processing queue
2. **Process**: Execute the job while it remains in the processing queue
3. **Success**: Remove job from processing queue using `LREM` (ACK)
4. **Failure**: Move job back to main queue using `RPOPLPUSH` (NACK)
5. **Recovery**: On startup, move all jobs from processing queue back to main queue

This ensures that:
- Jobs are never lost even if the worker crashes
- Jobs can be retried automatically
- Multiple workers can safely process jobs concurrently

## Error Handling

- Failed jobs are automatically moved back to the main queue for retry
- Instances are automatically returned even if processing fails
- Detailed error logging for debugging
- Graceful handling of network failures and timeouts

## Development

### Run tests

```bash
cargo test
```

### Build for production

```bash
cargo build --release
```

### Format code

```bash
cargo fmt
```

### Lint

```bash
cargo clippy
```

## License

MIT

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.
