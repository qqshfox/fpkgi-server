# FPKGi Server

A Rust-based server application for managing and serving PS4 package files (PKGs), generating JSON metadata, and providing an HTTP interface for browsing and downloading packages. This project was completely generated by Grok 3, an AI developed by xAI.

## Overview

FPKGi Server is designed to process PS4 package files, extract metadata, generate JSON files for package information, and serve them via an HTTP server. It includes features like filesystem watching for automatic regeneration of JSON files when packages change, and a simple web interface for directory browsing.

## Features

- **Package Processing**: Extracts metadata from PS4 PKG files and generates JSON files organized by category (games, updates, DLC, homebrew).
- **HTTP Server**: Serves package files and directory listings over HTTP with a configurable port.
- **Filesystem Watching**: Automatically regenerates JSON files when changes are detected in the packages directory.
- **Icon Extraction**: Optionally extracts icons from PKG files and serves them alongside the packages.
- **External JSON Merging**: Merges external JSON files with generated package data for additional metadata or customization.
- **Logging**: Comprehensive logging with configurable levels (info, debug, error) using the `log` crate and `env_logger`.
- **Cross-Platform Support**: Runs on macOS (Intel and Apple Silicon), Linux (x86_64 and ARM64), and Windows (x86_64).
- **Docker Support**: Multi-architecture Docker images (linux/x86_64, linux/arm64) available via GitHub Container Registry.

## Prerequisites

- Rust (stable, edition 2021) - [Install Rust](https://www.rust-lang.org/tools/install)
- Cargo (comes with Rust)
- Docker (optional, for containerized deployment)

## Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/qqshfox/fpkgi-server.git
   cd fpkgi-server
   ```

2. Build the project:
   ```bash
   cargo build --release
   ```

3. The executable will be available at `target/release/fpkgi-server`. Alternatively, download pre-built binaries from the [GitHub Releases](https://github.com/qqshfox/fpkgi-server/releases) or pull the Docker image from GHCR:
   ```bash
   docker pull ghcr.io/qqshfox/fpkgi-server:latest
   ```

   **Note for macOS Users**: If you download a binary and see a "malware" warning (e.g., “Apple could not verify ‘fpkgi-server-x86_64-apple-darwin’ is free of malware”), bypass it by:
   - Right-click the binary > "Open" (allows an "Open" button).
   - Or run in Terminal: `xattr -d com.apple.quarantine ./fpkgi-server-x86_64-apple-darwin` followed by `chmod +x ./fpkgi-server-x86_64-apple-darwin` and `./fpkgi-server-x86_64-apple-darwin`.

## Usage

FPKGi Server supports several commands via CLI arguments using `clap`. Below are examples using the `fpkgi-server` binary, with `host` as the primary command:

### Host (All-in-One)

Run a server, generate JSONs, and regenerate on package changes:

```bash
fpkgi-server host --port 8080 --packages "/path/to/packages:pkgs" --url "http://example.com" --out "/path/to/output:jsons" --icons "/path/to/icons:icons"
```

- Combines serving, generating, and watching functionality

### Generate JSON Files

Generate JSON metadata from PS4 package files:

```bash
fpkgi-server generate --packages "/path/to/packages:pkgs" --url "http://example.com" --out "/path/to/output:jsons" --icons "/path/to/icons:icons" --external "/path/to/external"
```

- `--packages`: Directory containing PKG files (format: `fs_path:url_path`)
- `--url`: Base URL for package links
- `--out`: Output directory for JSON files (format: `fs_path:url_path`)
- `--icons`: Optional directory for extracted icons (format: `fs_path:url_path`)
- `--external`: Optional directory with JSON files to merge into package data (recursive merge with `{"DATA":{}}` structure)

### Serve Directories

Start an HTTP server to serve directories:

```bash
fpkgi-server serve --dirs "packages:/path/to/packages" --dirs "jsons:/path/to/jsons" --port 8080
```

- `--dirs`: List of directories to serve (format: `name:path`)
- `--port`: Port to run the server on (default: 8000)

### Watch Directories

Watch directories for changes and log events:

```bash
fpkgi-server watch --dirs "/path/to/packages"
```

- `--dirs`: List of directories to watch

### Logging

Control log verbosity with the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug fpkgi-server host --port 8080 --packages "/path/to/packages:pkgs" --url "http://example.com" --out "/path/to/output:jsons"
```

- Levels: `error`, `warn`, `info`, `debug` (default: `info`)

## Docker Usage

### Docker Image

Pull and run the multi-arch Docker image from GitHub Container Registry:

```bash
docker run -p 8000:8000 -v ./packages:/data/packages -v ./jsons:/data/jsons -v ./icons:/data/icons ghcr.io/qqshfox/fpkgi-server:latest
```

### Docker Compose

Use `docker-compose.yml` for local deployment:

```bash
docker-compose up --build
```

- Configures volumes for packages, JSONs, and icons, and runs the `host` subcommand.

## Systemd Service

Deploy as a systemd service on Linux:

1. Copy the binary to `/usr/local/bin/`:
   ```bash
   sudo cp target/release/fpkgi-server /usr/local/bin/
   ```

2. Install `fpkgi-server.service` to `/etc/systemd/system/` (see file for details), then:
   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable fpkgi-server.service
   sudo systemctl start fpkgi-server.service
   ```

## Project Structure

```
fpkgi-server/
├── Cargo.toml          # Dependencies and project metadata
├── Dockerfile          # Multi-arch Docker image definition
├── docker-compose.yml  # Docker Compose configuration
├── fpkgi-server.service# Systemd service configuration
├── LICENSE             # MIT License
├── README.md           # Project documentation
├── docs/
│   ├── sorting.md      # Sorting behavior documentation
│   └── 404_resolution.md # 404 resolution process documentation
└── src/
    ├── main.rs         # Entry point and CLI parsing
    ├── args.rs         # Command-line argument definitions
    ├── enums.rs        # Category enumerations
    ├── json_builder.rs # JSON generation logic
    ├── ps4_package.rs  # PS4 package file processing
    ├── server.rs       # HTTP server implementation
    ├── sfo_processor.rs# SFO file parsing
    ├── utils.rs        # Utility functions
    └── watcher.rs      # Filesystem watching
```

## Building for Multiple Platforms

The GitHub Actions workflow (`build.yml`) builds binaries and a multi-arch Docker image (linux/x86_64, linux/arm64) for macOS (Intel and Apple Silicon), Linux (x86_64 and ARM64), and Windows (x86_64). Binaries are released as GitHub Release assets, and the Docker image is pushed to `ghcr.io/qqshfox/fpkgi-server`.

## Dependencies

- `actix-web` - HTTP server framework
- `actix-files` - Static file serving
- `anyhow` - Error handling
- `serde_json` - JSON serialization
- `tokio` - Async runtime
- `clap` - CLI parsing
- `log` & `env_logger` - Logging
- `notify` - Filesystem events
- See `Cargo.toml` for full list

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/YourFeature`)
3. Commit your changes (`git commit -am "Add your feature"`)
4. Push to the branch (`git push origin feature/YourFeature`)
5. Open a Pull Request

Please ensure your code follows Rust conventions and includes appropriate tests.

## Credits

This project builds upon concepts and ideas from [PS4 PKG Tool](https://github.com/mc-17/ps4_pkg_tool) by [mc-17](https://github.com/mc-17). Special thanks for their foundational work on PS4 package handling.

## About

This project was completely generated by Grok 3, an AI developed by xAI. It showcases the capabilities of AI-driven code generation for complex Rust applications.

## Documentation

- [Sorting Behavior](docs/sorting.md) - Details on how directories and files are sorted in the web interface.
- [404 Resolution Process](docs/404_resolution.md) - Chronicles the process to resolve 404 errors and directory listing issues.

## Disclaimer

As this project was generated by an AI (Grok 3), it may contain bugs or unexpected behavior due to the limitations of current AI capabilities. Please test thoroughly and report any issues you encounter.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgements

- Built with Rust and the amazing ecosystem of crates
- Inspired by PS4 package management needs
