# Contributing to Baud

Thank you for your interest in contributing to Baud! This document provides guidelines for contributing.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/<your-username>/Baud.git`
3. Create a branch: `git checkout -b feature/your-feature`
4. Make your changes
5. Run tests: `cargo test`
6. Push and open a Pull Request

## Development Setup

- **Rust**: Install via [rustup](https://rustup.rs/) (stable toolchain)
- **Build**: `cargo build`
- **Test**: `cargo test`
- **Lint**: `cargo clippy`
- **Format**: `cargo fmt`

## Code Style

- Run `cargo fmt` before committing
- All code must pass `cargo clippy` without warnings
- Add tests for new functionality
- Keep functions focused and small

## Architecture

Baud is organized as a Cargo workspace with 8 crates:

| Crate | Purpose |
|-------|---------|
| `baud-core` | Types, state machine, cryptography |
| `baud-consensus` | BFT consensus protocol |
| `baud-network` | P2P networking layer |
| `baud-api` | REST API (Axum) with rate limiting |
| `baud-cli` | Command-line interface |
| `baud-node` | Full node binary |
| `baud-storage` | Persistent storage (sled) |
| `baud-wallet` | Encrypted wallet (AES-256-GCM + Argon2id) |

## Pull Request Process

1. Ensure all tests pass (`cargo test`)
2. Update documentation if you changed public APIs
3. Add a clear description of your changes
4. Link any related issues

## Reporting Issues

- Use GitHub Issues for bug reports and feature requests
- Include reproduction steps for bugs
- Include your Rust version (`rustc --version`)

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
