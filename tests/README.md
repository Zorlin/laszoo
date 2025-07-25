# Laszoo Test Suite

This directory contains comprehensive integration tests for Laszoo. All tests use real file operations, real git commands, and simulate real MooseFS behavior.

## Test Structure

- `common/mod.rs` - Test utilities and environment setup
- `enrollment_test.rs` - Tests for file and directory enrollment
- `template_test.rs` - Tests for Handlebars and quack tag processing
- `sync_test.rs` - Tests for synchronization and watch mode
- `git_test.rs` - Tests for git integration and commit behavior
- `group_test.rs` - Tests for group management operations
- `integration_test.rs` - Full workflow integration tests

## Running Tests

```bash
# Build first (required)
cargo build --release

# Run all tests
./run_tests.sh

# Run specific test file
cargo test --release enrollment_test -- --test-threads=1

# Run specific test
cargo test --release test_enroll_single_file -- --test-threads=1

# Run with debug output
RUST_LOG=laszoo=debug cargo test --release -- --test-threads=1
```

## Test Environment

Each test creates an isolated environment in `/tmp/laszoo-test-*` with:
- A simulated MooseFS mount point
- A temporary hostname
- A git repository
- Isolated file system

Tests clean up after themselves automatically.

## Important Notes

1. **No Mocks**: All tests use real operations - no mocking of file systems, git, or any other components
2. **Sequential Execution**: Tests run with `--test-threads=1` to avoid conflicts
3. **Release Mode**: Tests must run against release build for proper timing
4. **Root Access**: Some tests may require root access for certain operations
5. **MooseFS**: Tests simulate MooseFS behavior but don't require actual MooseFS installation

## Writing New Tests

Use the `TestEnvironment` from `common/mod.rs`:

```rust
#[test]
fn test_my_feature() {
    let env = TestEnvironment::new("my_feature");
    env.setup_git().expect("Failed to setup git");
    
    // Create test files
    let file = env.create_test_file("test.conf", "content");
    
    // Run laszoo commands
    let output = env.run_laszoo(&["enroll", "group", file.to_str().unwrap()])
        .expect("Failed to run laszoo");
    
    assert!(output.status.success());
    
    // Environment automatically cleans up
}
```

## Test Coverage

The test suite covers:
- File and directory enrollment
- Template processing (Handlebars and quack tags)
- Synchronization strategies (converge, rollback, freeze, drift)
- Multi-machine coordination
- Git integration and commit handling
- Group management
- Watch mode behavior
- Conflict resolution
- Edge cases and error conditions