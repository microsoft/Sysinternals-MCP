# Sysinternals MCP Server

A Rust implementation of a Model Context Protocol (MCP) server that provides access to Windows debug output capture functionality, similar to the [Sysinternals DebugView](https://learn.microsoft.com/en-us/sysinternals/downloads/debugview) tool.

## Features

- **Session-based capture**: Create multiple independent capture sessions with their own filters and read positions
- **Flexible filtering**: Filter debug output by regex patterns (include/exclude), process names, or PIDs
- **Ring buffer**: Efficient 100K entry ring buffer (configurable via `DBGVIEW_BUFFER_SIZE` environment variable)
- **Lazy capture**: Debug capture only starts when the first session is created, minimizing overhead
- **Local capture**: Captures debug output from the local machine only (non-elevated)
- **Process listing**: Built-in tool to list running processes for filter setup

## MCP Tools

| Tool | Description |
|------|-------------|
| `create_session` | Create a new debug capture session |
| `destroy_session` | Destroy a session and free resources |
| `list_sessions` | List all active capture sessions |
| `get_session_status` | Get detailed session status including filter info |
| `set_filters` | Set include/exclude patterns and process filters |
| `get_output` | Retrieve captured debug output from a session |
| `clear_session` | Clear the session's read position |
| `list_processes` | List running processes with optional name filter |

## Installation

### Pre-built Binary

Download the latest release from the [Releases](../../releases) page.

### Claude Desktop Configuration

Add to your `claude_desktop_config.json` (located at `%APPDATA%\Claude\claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "sysinternals": {
      "command": "C:\\path\\to\\sysinternals-mcp.exe"
    }
  }
}
```

### VS Code with Copilot Configuration

Add to your VS Code settings (`.vscode/mcp.json` or user settings):

```json
{
  "mcp": {
    "servers": {
      "sysinternals": {
        "command": "C:\\path\\to\\sysinternals-mcp.exe"
      }
    }
  }
}
```

## Building from Source

### Prerequisites

- [Rust](https://rustup.rs/) (1.75 or later recommended)
- Windows 10/11 (for debug output capture functionality)

### Build Steps

```powershell
# Clone the repository
git clone https://github.com/YOUR_USERNAME/sysinternals-mcp.git
cd sysinternals-mcp

# Build release binary
cargo build --release

# The binary will be at target/release/sysinternals-mcp.exe
```

### Development Build

```powershell
# Build with debug symbols
cargo build

# Run directly for testing
cargo run
```

## Project Structure

```
sysinternals-mcp/
├── Cargo.toml              # Workspace configuration
├── README.md               # This file
├── LICENSE                 # MIT License
├── server/                 # MCP server binary
│   ├── Cargo.toml
│   └── src/
│       └── main.rs         # Server entry point and MCP tool implementations
└── tools/
    ├── dbgview/            # Debug output capture library
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs      # Library exports
    │       ├── capture.rs  # Windows debug output capture
    │       ├── error.rs    # Error types
    │       ├── filter.rs   # Regex-based filtering
    │       ├── process.rs  # Process enumeration
    │       ├── ring_buffer.rs  # Thread-safe ring buffer
    │       └── session.rs  # Session management
    └── test_debug_app/     # Test application for generating debug output
        ├── Cargo.toml
        └── src/
            └── main.rs     # Generates OutputDebugString calls for testing
```

## Usage Examples

### Basic Workflow

1. **Create a capture session**:
   ```
   create_session(name: "my-session")
   → Returns: { session_id: "abc123", name: "my-session" }
   ```

2. **Set up filters** (optional):
   ```
   set_filters(
     session_id: "abc123",
     include_patterns: ["ERROR", "WARNING"],
     process_names: ["myapp"]
   )
   ```

3. **Get debug output**:
   ```
   get_output(session_id: "abc123", limit: 100)
   → Returns: [{ seq: 1, time: "12:34:56.789", pid: 1234, process_name: "myapp.exe", text: "ERROR: ..." }, ...]
   ```

4. **Clean up**:
   ```
   destroy_session(session_id: "abc123")
   ```

### Finding Processes to Filter

```
list_processes(name_filter: "notepad")
→ Returns: [{ pid: 5678, name: "notepad.exe" }, ...]
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DBGVIEW_BUFFER_SIZE` | `100000` | Maximum entries in the ring buffer |
| `RUST_LOG` | `info` | Log level (trace, debug, info, warn, error) |

## How It Works

The server captures debug output using the same Windows kernel objects as Sysinternals DebugView:

1. **DBWinMutex**: Mutex to coordinate access with other debug monitors
2. **DBWIN_BUFFER**: Shared memory buffer where debug strings are written
3. **DBWIN_BUFFER_READY**: Event signaled when buffer is ready for new data
4. **DBWIN_DATA_READY**: Event signaled when new debug data is available

When an application calls `OutputDebugString`, Windows writes the message to the shared buffer and signals `DBWIN_DATA_READY`. The capture thread reads the message, stores it in the ring buffer, and signals `DBWIN_BUFFER_READY` to allow the next message.

## Limitations

- **Windows only**: Debug output capture uses Windows-specific APIs
- **Local capture only**: Does not support remote machine capture
- **Non-elevated capture**: Cannot capture kernel-mode debug output (requires elevation and kernel debugger setup)
- **Single debugger**: Only one application can capture debug output at a time

## Troubleshooting

### "Another debugger is attached"

Another application (such as Visual Studio, DebugView, or another instance of this server) is already capturing debug output. Close the other application and try again.

### No output captured

- Ensure the target application is calling `OutputDebugString`
- Check that filters aren't too restrictive
- Verify the capture session was created successfully

### Missing process names

Process name resolution may fail for short-lived processes or those with restricted access. The PID will still be available.

## License

MIT License - see [LICENSE](LICENSE) for details.

## Related Projects

- [dbgview-mcp](https://github.com/markrussinovich/dbgview-mcp) - Python implementation with C capture component
- [Sysinternals DebugView](https://learn.microsoft.com/en-us/sysinternals/downloads/debugview) - The original GUI tool

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
