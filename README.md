<div align="center">
  <img style="width: 196px;" src="https://raw.githubusercontent.com/nowayhecodes/rpm/main/assets/logo.webp" alt="RPM Logo" />
</div>

<div align="center">
  <h1>RPM</h1>
</div>

A high-performance Node.js package manager implemented in Rust, focused on speed, safety, and efficiency. RPM handles package installation, dependency resolution, and version control while maintaining compatibility with the Node.js ecosystem.

### Features

- Fast parallel package downloads and installations
- Secure package verification with hash checking
- Support for global and local package installations
- Full compatibility with `package.json` and npm registry
- Memory-safe implementation leveraging Rust's guarantees
- Efficient dependency resolution and version management

### Installation

#### From Source

```bash
git clone https://github.com/nowayhecodes/rpm
cd rpm
cargo install --path .
```

#### From Cargo

```bash
cargo install rpm
```

#### Quick Install (Unix-like systems)

```bash
# Default installation
curl -fsSL https://raw.githubusercontent.com/nowayhecodes/rpm/main/install.sh | sh

# Custom installation directory
export RPM_INSTALL_DIR="$HOME/.local/bin"
curl -fsSL https://raw.githubusercontent.com/nowayhecodes/rpm/main/install.sh | sh
```

⚠️ Always inspect installation scripts before running them with root privileges. You can view the script [here](https://raw.githubusercontent.com/nowayhecodes/rpm/main/install.sh).

### Usage

#### Installing Packages

Install packages locally (in current project):
```bash
rpm install express
rpm install lodash react react-dom
```

Install packages globally:
```bash
rpm install -g typescript
```

#### Removing Packages

Remove local packages:
```bash
rpm remove express
```

Remove global packages:
```bash
rpm remove -g typescript
```

#### Configuration

RPM uses the standard `package.json` for project configuration and is fully compatible with existing Node.js projects. It respects:

- Dependencies and devDependencies
- Version constraints
- Package scripts
- Other npm-compatible configurations

### Performance

RPM leverages Rust's concurrency model and safety features to provide:

- Parallel package downloads
- Efficient disk I/O operations
- Minimal memory footprint
- Fast dependency resolution

### Security

- Package integrity verification using SHA-256 checksums
- Secure downloads over HTTPS
- Sandboxed package installations
- Memory-safe operations

### Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Requirements

- Rust 1.70 or higher
- Node.js environment (for compatibility)

### License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

### Acknowledgments

- NPM team for the package registry and ecosystem
- Rust community for excellent async tools and libraries
- Contributors and users of this project

### Support

For bugs, questions, and discussions please use the [GitHub Issues](https://github.com/nowayhecodes/rpm/issues).
