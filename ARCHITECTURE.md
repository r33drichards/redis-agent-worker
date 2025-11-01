# Redis Agent Worker Architecture

## Overview

The Redis Agent Worker uses **Hyperlight** to run AI agents in a secure, sandboxed environment. This architecture ensures that agents have strictly limited permissions and can only access resources explicitly granted to them.

## Key Components

### 1. Host Application (`src/`)

The host application manages the lifecycle of agent jobs and provides controlled access to external resources.

**Main Components:**
- `main.rs`: CLI entry point and configuration
- `worker.rs`: Job processing loop and orchestration
- `agent.rs`: Hyperlight sandbox management and host function registration
- `queue.rs`: Redis-based reliable queue
- `instance.rs`: Instance allocation from external allocator API
- `git.rs`: Git repository operations

### 2. Guest Binary (`guest/`)

The guest is a **no_std** Rust library that runs inside the Hyperlight sandbox. It has no direct access to the filesystem, network, or system calls. All I/O must go through host functions.

**Key Features:**
- Runs in isolated microVM
- No direct network or filesystem access
- Communicates with host via function calls
- Implements agent logic using host-provided primitives

### 3. Security Model

#### Network Access Restrictions

The guest has **zero direct network access**. All network operations are mediated by the host:

1. **MCP Server URL Whitelisting**: When a job specifies an MCP connection URL, only that specific server can be accessed.
2. **Host Function Validation**: Every network request is validated against the allowed MCP URL before execution.
3. **Blocked by Default**: If no MCP URL is configured, all network access is blocked.

```rust
// Host validates every network request
if url.host_str() != allowed_url.host_str() {
    error!("Blocked unauthorized connection attempt");
    return Err(anyhow::anyhow!("Unauthorized network access"));
}
```

#### File System Access

- Guest can read/write files only in the configured working directory
- Configured via `SandboxConfiguration::set_working_directory()`
- Cannot escape the working directory

### 4. Host Functions

The host provides the following functions that the guest can call:

#### `InitializeMCPConnection(url: String) -> Void`
Validates that the provided URL matches the allowed MCP server.

#### `GetMCPTools() -> String`
Fetches the list of available tools from the MCP server (returns JSON).

#### `ExecuteMCPTool(tool_name: String, arguments: String) -> String`
Executes a tool on the MCP server with the given arguments (JSON).

### 5. Agent Execution Flow

```
1. Job arrives in Redis queue
   ↓
2. Worker borrows instance from allocator
   ↓
3. Worker clones git repository
   ↓
4. Worker creates Hyperlight sandbox
   ↓
5. Worker registers host functions with allowed MCP URL
   ↓
6. Guest ExecuteAgent function is called
   ↓
7. Guest calls host functions for network I/O
   ↓
8. Host validates and proxies requests to MCP server
   ↓
9. Guest returns result to host
   ↓
10. Worker commits and pushes changes
   ↓
11. Worker returns instance to allocator
```

## Why Hyperlight Instead of Running AgentAI Directly?

The original implementation tried to run Hyperlight as an external process, which missed the point of Hyperlight's security model:

### Problems with Direct Execution:
- ❌ Agent has full system access
- ❌ Cannot restrict network access to specific URLs
- ❌ Difficult to sandbox filesystem operations
- ❌ No memory isolation
- ❌ Trust-based security (env variables can be bypassed)

### Hyperlight Solution:
- ✅ Agent runs in isolated microVM with no system access
- ✅ All I/O goes through validated host functions
- ✅ Network access enforced at host level (cannot be bypassed)
- ✅ Memory isolated between host and guest
- ✅ Security enforced by hypervisor, not trust

### AgentAI Compatibility

AgentAI requires a full `std` environment with async/networking, but Hyperlight guests are `no_std`. The solution:

- **Guest**: Implements agent logic pattern using host function calls
- **Host**: Provides networking primitives with security enforcement
- **Result**: Same agent behavior with strong security guarantees

## Building

### Build Guest:
```bash
cd guest
./build.sh
```

### Build Host:
```bash
cargo build --release
```

### Run Worker:
```bash
export GUEST_BINARY_PATH=./guest/target/release/libagent_guest.so
export REDIS_URL=redis://localhost:6379
export ALLOCATOR_API_URL=http://localhost:8080

cargo run --release -- run
```

## Configuration

### Environment Variables:

- `REDIS_URL`: Redis connection URL (default: `redis://127.0.0.1:6379`)
- `QUEUE_NAME`: Queue name (default: `agent_jobs`)
- `ALLOCATOR_API_URL`: Instance allocator API URL (default: `http://localhost:8080`)
- `GUEST_BINARY_PATH`: Path to guest binary (default: `./target/release/libagent_guest.so`)
- `WORK_DIR`: Working directory for repos (default: `/tmp/agent-worker`)
- `LOG_LEVEL`: Logging level (default: `info`)

## Security Guarantees

1. **Network Isolation**: Guest can only communicate with the MCP server URL provided by the allocator
2. **Filesystem Isolation**: Guest can only access files in the configured working directory
3. **Memory Isolation**: Guest memory is isolated from host memory
4. **No System Calls**: Guest cannot make system calls directly
5. **Resource Limits**: Hyperlight enforces CPU and memory limits on the guest

## Future Improvements

- [ ] Add support for multiple MCP servers
- [ ] Implement AgentAI library integration in guest (when std support available)
- [ ] Add metrics and monitoring
- [ ] Implement guest code signing
- [ ] Add support for custom host functions per job
