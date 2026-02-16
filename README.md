# DATEX (Native)

This crate contains the DATEX library for native platforms, as well as the DATEX Command Line Interface (CLI) for interacting with the DATEX runtime.

The DATEX CLI provides a REPL (Read-Eval-Print Loop) for executing DATEX code interactively and a command to run DATEX files.

## Installation
The DATEX CLI can be installed on various platforms. Below are the installation methods for different operating systems.

### Brew

You can install the DATEX CLI using Homebrew:
```bash
brew install unyt-org/datex-native/datex
```

### Install Script
You can install the DATEX CLI using the provided installation script. This script will download and install the latest version of the DATEX CLI.
```bash
curl -fsSL https://raw.githubusercontent.com/unyt-org/datex-native/refs/heads/main/install.sh | sh
```

To select a specific version for the installation, you can pass the tag as an argument:
```bash
curl -fsSL https://raw.githubusercontent.com/unyt-org/datex-native/refs/heads/main/install.sh | sh -s -- v0.1.0
```
### From source
Alternatively, you can build the DATEX CLI from source using Cargo, the Rust package manager. Make sure you have Rust and Cargo installed, then run:
```bash
cargo build --release
```

## Usage

### Running the REPL
```shell
datex
```

Alternatively, you can also use the `repl` subcommand:
```shell
datex repl
```

To show debug logs, run the `repl` subcommand with the `--verbose` or `-v` flag:
```shell
datex repl -v
```

To start the repl with a specific DATEX configuration file, use the `--config` or `-c` flag:
```shell
datex repl --config path/to/config.dx
```

### Running a DATEX file
```shell
datex run path/to/file.dx
```

## Development
### Running the REPL
```shell
cargo run
```

### Running the Workbench
```shell
cargo run workbench
```

### Building for Release
```shell
cargo build --release
./target/release/datex_cli
```

---

<sub>&copy; unyt 2025 • [unyt.org](https://unyt.org)</sub>
