# 🧊 Freeze

> A modern CLI tool written in Rust to snapshot and restore your files with style.

[![Made with Rust](https://img.shields.io/badge/Made%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Features

- Take snapshots of files and directories
- Keep track of file changes over time
- Restore previous versions easily
- Search through your snapshots
- Compare snapshots with diff view
- Inspect file evolution across snapshots
- Web interface for browsing snapshots
- Lightning-fast operations
- Selective file exclusions
- Efficient storage management
- **MCP (Model Context Protocol) support** - Use freeze with AI assistants

## Installation

```bash
cargo install freeze
```

## Usage

### Basic Commands

```bash
# Save a file or directory state
freeze save <path>

# Restore from a snapshot
freeze restore <path>

# List all snapshots
freeze ls

# List snapshots in current directory
freeze cls

# Search snapshots
freeze search <pattern>

# Check file status
freeze check <path>
```

### Advanced Features

```bash
# Export a snapshot
freeze export <snapshot_path> [-d destination]

# View snapshot contents
freeze view <snapshot_path>

# Compare two snapshots with diff
freeze diff <checksum1> <checksum2> [path]
# Or compare snapshot with current file
freeze diff <checksum> current [path]

# Inspect file evolution across snapshots
freeze inspect <path>

# Start web interface
freeze web [--port <port>]

# Manage exclusions
freeze exclusion add <pattern> <type>
freeze exclusion remove <pattern>
freeze exclusion list

# Clear snapshots
freeze clear [--all] [path]
```

### MCP (AI Assistant Integration)

Freeze can be used as an MCP server, allowing AI assistants to interact with your snapshots.

```bash
# Start the MCP server
freeze mcp
```

#### Available MCP Tools

| Tool | Description |
|------|-------------|
| `freeze_save` | Save a snapshot of a file or directory |
| `freeze_restore` | Restore from a snapshot (use `checksum` param to select specific snapshot) |
| `freeze_list` | List all snapshots with IDs and checksums |
| `freeze_list_directory` | List snapshots in current directory |
| `freeze_search` | Search snapshots by pattern |
| `freeze_check` | Check if files have changed |
| `freeze_view` | View snapshot contents |
| `freeze_export` | Export a snapshot |
| `freeze_clear` | Clear snapshots |
| `freeze_snapshot_info` | Get detailed info about a specific snapshot |
| `freeze_compare` | Compare two snapshots or snapshot vs current file |
| `freeze_exclusion_add` | Add an exclusion pattern |
| `freeze_exclusion_list` | List exclusion patterns |
| `freeze_exclusion_remove` | Remove an exclusion pattern |

#### MCP Usage Example

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "freeze_list",
    "arguments": {}
  }
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "Snapshots:\nID      | Date/Time                      | Size      | Checksum            | Path\n────────────────────────────────────────────────────────────────────────────────\n1       | 2024-01-15T10:30:00            | 13 B      | a1fff0ff12345678    | test_file.txt\n..."
      }
    ]
  }
}
```

#### Restore Specific Snapshot

```json
{
  "method": "tools/call",
  "params": {
    "name": "freeze_restore",
    "arguments": {
      "path": "/path/to/file.txt",
      "checksum": "a1fff0ff"
    }
  }
}
```

#### Compare Snapshots

```json
{
  "method": "tools/call",
  "params": {
    "name": "freeze_compare",
    "arguments": {
      "path": "/path/to/file.txt",
      "source": "checksum1",
      "target": "checksum2"
    }
  }
}
```

Or compare snapshot with current file:
```json
{
  "method": "tools/call",
  "params": {
    "name": "freeze_compare",
    "arguments": {
      "path": "/path/to/file.txt",
      "source": "checksum1",
      "target": "current"
    }
  }
}
```

## Configuration

Freeze automatically stores its data in `~/.freeze/data.sql`. You can manage file exclusions using the `exclusion` commands.

## Examples

```bash
# Save your project
freeze save ./my-project

# Check what's changed
freeze check ./my-project

# Restore a specific file
freeze restore ./my-project/src/main.rs

# Export a snapshot
freeze export ./my-project/config.json -d ./backup
```

## Contributing

Contributions are welcome! Feel free to:
- Report bugs
- Suggest features
- Submit pull requests

## License

[CC BY-NC 4.0](https://creativecommons.org/licenses/by-nc/4.0/)

## Author

**Nils Begou**
- Portfolio: [nils.begou.dev](https://nils.begou.dev)

---

Made with Rust
