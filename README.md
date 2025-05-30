# coolclis

A Rust CLI tool to download and install binary tools from GitHub releases pages.

## Features

- Automatically detects your operating system and architecture
- Downloads the appropriate binary for your platform
- Handles common archive formats (zip, tar.gz)
- Shows download progress
- Installs to your local bin directory (~/.local/bin by default)
- Manages a list of predefined tools for quick installation

## Installation

### From source

```bash
cargo install --path .
```

## Usage

Install a tool from a GitHub repository:

```bash
# Install the latest version of a tool
coolclis install owner/repo --bin tool_name

# Install a specific version
coolclis install owner/repo --bin tool_name --version v1.2.3

# Install to a custom directory
coolclis install owner/repo --bin tool_name --dir /usr/local/bin

# Install a predefined tool by its name
coolclis install tool_name
```

List all predefined tools:

```bash
coolclis list
```

Add a new tool to the configuration:

```bash
# Add a new tool with description
coolclis add tool_name owner/repo --description "Description of the tool"

# Add a new tool without description
coolclis add tool_name owner/repo
```

### Examples

```bash
# Install the latest version of ripgrep
coolclis install BurntSushi/ripgrep --bin rg

# Install a specific version of bat
coolclis install sharkdp/bat --bin bat --version v0.22.1

# Install fd-find to a custom directory
coolclis install sharkdp/fd --bin fd --dir ~/bin

# Add a new tool to the configuration
coolclis add tokei XAMPPRocky/tokei --description "Count lines of code quickly"

# Install a predefined tool
coolclis install ripgrep
```

## How it works

1. Fetches release information from the GitHub API
2. Finds the appropriate asset for your platform
3. Downloads the asset with a progress bar
4. If it's an archive (zip, tar.gz), extracts it
5. Installs the binary to the specified directory
6. Makes the binary executable

## Supported platforms

- Linux (x86_64, arm64)
- macOS (x86_64, arm64)
- Windows (x86_64)

## License

MIT 