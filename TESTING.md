# Testing Guidelines

## Overview

This project requires **minimum 80% code coverage** for all Rust code. Git commits are automatically blocked if tests fail or coverage is below 80%.

## Running Tests

### Basic test commands

```bash
# Run all tests
cd src-tauri
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture
```

### Coverage commands

```bash
# Generate HTML coverage report
cd src-tauri
cargo llvm-cov --html --output-dir target/coverage

# Open coverage report in browser
cargo llvm-cov --html --open

# Check if coverage meets 80% minimum
cargo llvm-cov --fail-under-lines 80

# Automated script (runs tests + coverage)
./test-coverage.sh
```

## Pre-commit Hook

A Git pre-commit hook automatically runs on every commit to ensure:
1. ✅ All tests pass
2. ✅ Code coverage >= 80%

If either check fails, the commit is **rejected**.

### Bypassing the hook (NOT recommended)

Only in emergency situations:
```bash
git commit --no-verify -m "message"
```

## Writing Tests

### Test structure

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    #[test]
    fn test_something() {
        let conn = setup_test_db();
        let result = my_function(&conn);
        assert!(result.is_ok());
    }
}
```

### Test helpers available

- `setup_test_db()` - Creates in-memory SQLite database with schema
- `create_test_triples()` - Returns sample test data
- `assert_triple_exists(conn, subject, predicate)` - Assertion helper
- `get_active_triple_count(conn)` - Count triples in test DB

## Coverage Requirements by Module

| Module | Current Coverage | Target |
|--------|-----------------|--------|
| eavto/stats | ~90% | 80%+ ✅ |
| eavto/store | TBD | 80%+ |
| eavto/query | TBD | 80%+ |
| eavto/connection | TBD | 80%+ |

## Troubleshooting

### Tests fail locally but pass in CI
- Ensure database is clean: `rm FOUNDATION.db`
- Check for platform-specific issues

### Coverage report shows 0%
- Make sure `cargo-llvm-cov` is installed: `cargo install cargo-llvm-cov`
- Ensure `llvm-tools-preview` component is installed: `rustup component add llvm-tools-preview`

### Pre-commit hook doesn't run
- Check if hook is executable: `chmod +x .git/hooks/pre-commit`
- Verify you're in a git repository

## CI/CD Integration

The pre-commit hook ensures local validation, but CI/CD should also run:
```yaml
- name: Run tests with coverage
  run: |
    cd src-tauri
    cargo llvm-cov --fail-under-lines 80
```
