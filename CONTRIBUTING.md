# Contributing to ReDash

Thank you for your interest in contributing to ReDash! Whether you are reporting a bug, proposing a new feature, improving documentation, or writing code, your contributions are welcome.

## Development Workflow

### Prerequisites

- **Rust**: Fixed workspace toolchain version **1.96.0** (check `rust-toolchain.toml`).
- **wasm-bindgen-cli**: Recommended for web client bundling (`cargo install wasm-bindgen-cli --version 0.2.105`).
- **macOS** (for GPUI desktop testing) or **Linux/Docker** (for headless server & web client).

### Setting Up

```bash
# Clone the repository
git clone https://github.com/reways/redash.git
cd redash

# Ensure the Rust toolchain is installed
rustup toolchain install 1.96.0 --component rustfmt --component clippy
rustup target add wasm32-unknown-unknown
```

### Verification & Testing Standards

All pull requests must pass the following strict automated checks before merging:

```bash
# 1. Code Formatting
cargo fmt --all -- --check

# 2. Strict Clippy (Zero Warnings)
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --target wasm32-unknown-unknown -p redash-web --all-targets -- -D warnings

# 3. Complete Test Suite
cargo test --workspace --locked
```

## Architecture & Code Principles

1. **Safety & Zero-Trust for Credentials**: Never store plaintext passwords or private key passphrases in unencrypted disk files. Always use system keychains or secure credential vaults.
2. **Honesty over Mocking**: Never fabricate simulated metrics when real probes fail. Always report the accurate error state and timestamp.
3. **Headless First**: Keep `redash-core` 100% headless and decoupled from GUI dependencies. Shared data structures belong in `redash-types`, and UI domain models belong in `redash-ui-core`.
4. **Resilient Remote File Operations**: Remote edits via SFTP must use atomic replacement (`posix-rename@openssh.com`), preserve file permissions, and retain original files or recovery backups upon failure.

## Submitting Pull Requests

1. Fork the repo and create your branch from `main`:
   ```bash
   git checkout -b feat/your-feature-name
   ```
2. Commit your changes following conventional commits:
   - `feat(...)`: New feature or capability
   - `fix(...)`: Bug fix
   - `refactor(...)`: Code refactoring without behavioral changes
   - `docs(...)`: Documentation improvements
   - `test(...)`: Adding or updating test cases
3. Push to your fork and submit a Pull Request targeting `main`.
4. Ensure CI checks pass cleanly.

## License

By contributing to ReDash, you agree that your contributions will be licensed under the [Apache-2.0 License](LICENSE).
