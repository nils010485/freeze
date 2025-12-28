# 🧊 Freeze

> A modern CLI tool written in Rust to snapshot and restore your files with style.

[![Made with Rust](https://img.shields.io/badge/Made%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Features

- Take snapshots of files and directories
- Keep track of file changes over time
- Restore previous versions easily
- Search through your snapshots
- Lightning-fast operations
- Selective file exclusions
- Efficient storage management

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

# Manage exclusions
freeze exclusion add <pattern> <type>
freeze exclusion remove <pattern>
freeze exclusion list

# Clear snapshots
freeze clear [--all] [path]
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
