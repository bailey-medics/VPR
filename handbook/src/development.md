# Development Tools

This project includes comprehensive Rust formatting and linting tools to maintain code quality.

## Quick Commands

```bash
# Format code
./scripts/fmt.sh
# or
cargo fmt --all

# Run linter
./scripts/lint.sh  
# or
cargo clippy --all-targets --all-features -- -D warnings

# Run all quality checks
./scripts/check-all.sh
```

## Pre-commit Hooks

The project uses pre-commit hooks to automatically check code quality before commits:

```bash
# Install pre-commit hooks
pre-commit install

# Run hooks manually on all files
pre-commit run --all-files

# Run specific hook
pre-commit run cargo-clippy

# Auto-fix formatting (manual stage)
pre-commit run cargo-fmt-fix --hook-stage manual
```

## Configuration Files

- **`rustfmt.toml`** - Code formatting configuration
- **`clippy.toml`** - Linting rules and thresholds
- **`.vscode/settings.json`** - VS Code settings for Rust development

## Available Hooks

1. **cspell** - Spell checking for code and comments
2. **cargo-fmt-check** - Formatting validation (runs on commit)
3. **cargo-clippy** - Linting and code analysis (runs on commit)
4. **cargo-check** - Compilation check (runs on commit)
5. **cargo-fmt-fix** - Auto-format code (manual stage only)
6. **cargo-test** - Run tests (manual stage only)

## VS Code Integration

The project includes VS Code settings that:

- Enable format-on-save with rustfmt
- Run clippy on save for real-time linting
- Configure proper Rust file associations
- Set up PATH for cargo/rustc tools
