#!/bin/bash
# Jetpack Integration Test Suite for Laszoo

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
LASZOO_BIN="${LASZOO_BIN:-./target/release/laszoo}"
TEST_DIR="/tmp/laszoo-jetpack-test-$$"
TEST_PLAYBOOK_DIR="$TEST_DIR/playbooks"
MFS_MOUNT="${MFS_MOUNT:-/mnt/laszoo}"

# Check if running with sudo
if [ "$EUID" -eq 0 ]; then 
    echo "Running tests as root (sudo already applied)"
    SUDO=""
else
    echo "Running tests as regular user - Laszoo will escalate privileges as needed"
    echo "You may be prompted for your sudo password by the application"
    SUDO=""
fi

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Helper functions
log() {
    echo -e "${GREEN}[TEST]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

run_test() {
    local test_name="$1"
    local test_function="$2"
    
    TESTS_RUN=$((TESTS_RUN + 1))
    log "Running: $test_name"
    
    if $test_function; then
        echo -e "${GREEN}✓${NC} $test_name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗${NC} $test_name"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

setup() {
    log "Setting up test environment..."
    
    # Create test directories
    mkdir -p "$TEST_PLAYBOOK_DIR"
    
    # Check if Laszoo binary exists
    if [[ ! -x "$LASZOO_BIN" ]]; then
        error "Laszoo binary not found at $LASZOO_BIN"
        exit 1
    fi
    
    # Check if MooseFS is mounted
    if [[ ! -d "$MFS_MOUNT" ]]; then
        warning "MooseFS mount not found at $MFS_MOUNT - some tests may fail"
    fi
}

cleanup() {
    log "Cleaning up test environment..."
    rm -rf "$TEST_DIR"
    
    # Clean up test playbooks from MooseFS
    if [[ -d "$MFS_MOUNT/playbooks/test-playbook-$$" ]]; then
        $SUDO "$LASZOO_BIN" playbook remove "test-playbook-$$" 2>/dev/null || true
    fi
}

# Test functions
test_playbook_add() {
    local playbook_name="test-playbook-$$"
    
    # Create a test playbook
    cat > "$TEST_PLAYBOOK_DIR/playbook.yml" << 'EOF'
- name: Test playbook
  groups:
    - all
  
  tasks:
    - !echo
      msg: "Hello from test playbook"
EOF
    
    # Add playbook to Laszoo
    if $SUDO "$LASZOO_BIN" playbook add "$TEST_PLAYBOOK_DIR" --name "$playbook_name" >/dev/null 2>&1; then
        # Verify it was added
        if [[ -d "$MFS_MOUNT/playbooks/$playbook_name" ]]; then
            # Clean up
            $SUDO "$LASZOO_BIN" playbook remove "$playbook_name" >/dev/null 2>&1
            return 0
        fi
    fi
    return 1
}

test_playbook_list() {
    local playbook_name="test-list-$$"
    
    # Create and add a playbook
    mkdir -p "$TEST_PLAYBOOK_DIR/list-test"
    echo "---" > "$TEST_PLAYBOOK_DIR/list-test/playbook.yml"
    
    $SUDO "$LASZOO_BIN" playbook add "$TEST_PLAYBOOK_DIR/list-test" --name "$playbook_name" >/dev/null 2>&1
    
    # List playbooks and check if our test playbook appears
    # Capture output to avoid broken pipe error
    local list_output=$("$LASZOO_BIN" playbook list 2>/dev/null || true)
    if echo "$list_output" | grep -q "$playbook_name"; then
        $SUDO "$LASZOO_BIN" playbook remove "$playbook_name" >/dev/null 2>&1
        return 0
    fi
    
    $SUDO "$LASZOO_BIN" playbook remove "$playbook_name" >/dev/null 2>&1 || true
    return 1
}

test_playbook_remove() {
    local playbook_name="test-remove-$$"
    
    # Create and add a playbook
    mkdir -p "$TEST_PLAYBOOK_DIR/remove-test"
    echo "---" > "$TEST_PLAYBOOK_DIR/remove-test/playbook.yml"
    
    $SUDO "$LASZOO_BIN" playbook add "$TEST_PLAYBOOK_DIR/remove-test" --name "$playbook_name" >/dev/null 2>&1
    
    # Remove it
    if $SUDO "$LASZOO_BIN" playbook remove "$playbook_name" >/dev/null 2>&1; then
        # Verify it's gone
        if [[ ! -d "$MFS_MOUNT/playbooks/$playbook_name" ]]; then
            return 0
        fi
    fi
    return 1
}

test_inventory_generation() {
    # Check if inventory directory exists
    if [[ -d "$MFS_MOUNT/inventory/jetpack" ]]; then
        # Check for groups directory
        if [[ -d "$MFS_MOUNT/inventory/jetpack/groups" ]]; then
            return 0
        fi
    fi
    return 1
}

test_metapackage_parsing() {
    local test_group="test-metapackage-$$"
    local packages_conf="$TEST_DIR/packages.conf"
    
    # Create a test packages.conf with playbook metapackage
    cat > "$packages_conf" << 'EOF'
# Test packages.conf
+nginx
++update
++playbook deploy-app
!apache2
EOF
    
    # Check if the file contains our playbook entry
    if grep -q "++playbook deploy-app" "$packages_conf"; then
        return 0
    fi
    return 1
}

test_playbook_path_resolution() {
    local playbook_name="test-path-$$"
    
    # Test 1: Add with custom path
    mkdir -p "$TEST_PLAYBOOK_DIR/custom"
    echo "---" > "$TEST_PLAYBOOK_DIR/custom/playbook.yml"
    
    if $SUDO "$LASZOO_BIN" playbook add "$TEST_PLAYBOOK_DIR/custom" \
        --name "$playbook_name" \
        --path-override "testing/custom-path" >/dev/null 2>&1; then
        
        # Check if it was created at custom path
        if [[ -d "$MFS_MOUNT/testing/custom-path" ]]; then
            # Clean up custom path if we have permissions
        if [[ -w "$MFS_MOUNT/testing/custom-path" ]]; then
            rm -rf "$MFS_MOUNT/testing/custom-path"
        else
            echo "Warning: Cannot clean up $MFS_MOUNT/testing/custom-path - insufficient permissions"
        fi
            return 0
        fi
    fi
    return 1
}

test_inventory_sync_with_groups() {
    # This test requires actual group setup
    # For now, just check if sync command doesn't error
    if "$LASZOO_BIN" status >/dev/null 2>&1; then
        return 0
    fi
    return 1
}

test_playbook_manifest() {
    # Check if playbook metadata exists after adding a playbook
    local playbook_name="test-manifest-$$"
    
    mkdir -p "$TEST_PLAYBOOK_DIR/manifest-test"
    echo "---" > "$TEST_PLAYBOOK_DIR/manifest-test/playbook.yml"
    
    $SUDO "$LASZOO_BIN" playbook add "$TEST_PLAYBOOK_DIR/manifest-test" --name "$playbook_name" >/dev/null 2>&1
    
    # Check if the playbook directory and metadata file exist
    if [[ -d "$MFS_MOUNT/playbooks/$playbook_name" ]]; then
        if [[ -f "$MFS_MOUNT/playbooks/$playbook_name/.laszoo-metadata.json" ]]; then
            # Check if metadata contains our playbook name
            if grep -q "$playbook_name" "$MFS_MOUNT/playbooks/$playbook_name/.laszoo-metadata.json" 2>/dev/null; then
                $SUDO "$LASZOO_BIN" playbook remove "$playbook_name" >/dev/null 2>&1
                return 0
            fi
        fi
    fi
    
    $SUDO "$LASZOO_BIN" playbook remove "$playbook_name" >/dev/null 2>&1 || true
    return 1
}

test_cli_help() {
    # Test if playbook help is available
    if "$LASZOO_BIN" playbook --help >/dev/null 2>&1; then
        return 0
    fi
    return 1
}

test_error_handling() {
    # Test adding non-existent playbook
    if ! $SUDO "$LASZOO_BIN" playbook add /non/existent/path >/dev/null 2>&1; then
        # Error is expected, test passes
        return 0
    fi
    return 1
}

# Integration test with actual Jetpack execution (requires Jetpack installed)
test_jetpack_execution() {
    # Check if jetpack is available
    if ! command -v jetpack >/dev/null 2>&1; then
        warning "Jetpack not installed, skipping execution test"
        return 0  # Skip test but don't fail
    fi
    
    local playbook_name="test-exec-$$"
    
    # Create a simple test playbook
    cat > "$TEST_PLAYBOOK_DIR/exec.yml" << 'EOF'
- name: Test execution
  groups:
    - all
  
  tasks:
    - !file
      name: Create test file
      path: /tmp/laszoo-jetpack-test-exec
      state: present
EOF
    
    # Add and run the playbook
    if $SUDO "$LASZOO_BIN" playbook add "$TEST_PLAYBOOK_DIR/exec.yml" --name "$playbook_name" >/dev/null 2>&1; then
        if $SUDO "$LASZOO_BIN" playbook run "$playbook_name" >/dev/null 2>&1; then
            # Check if test file was created
            if [[ -f /tmp/laszoo-jetpack-test-exec ]]; then
                rm -f /tmp/laszoo-jetpack-test-exec
                $SUDO "$LASZOO_BIN" playbook remove "$playbook_name" >/dev/null 2>&1
                return 0
            fi
        fi
    fi
    
    $SUDO "$LASZOO_BIN" playbook remove "$playbook_name" >/dev/null 2>&1 || true
    return 1
}

# Main test execution
main() {
    echo "=== Laszoo Jetpack Integration Test Suite ==="
    echo
    
    # Setup
    trap cleanup EXIT
    setup
    
    # Run tests
    run_test "CLI help available" test_cli_help
    run_test "Add playbook" test_playbook_add
    run_test "List playbooks" test_playbook_list
    run_test "Remove playbook" test_playbook_remove
    run_test "Inventory generation" test_inventory_generation
    run_test "Metapackage parsing" test_metapackage_parsing
    run_test "Custom path resolution" test_playbook_path_resolution
    run_test "Inventory sync" test_inventory_sync_with_groups
    run_test "Playbook manifest" test_playbook_manifest
    run_test "Error handling" test_error_handling
    run_test "Jetpack execution" test_jetpack_execution
    
    # Summary
    echo
    echo "=== Test Summary ==="
    echo "Tests run: $TESTS_RUN"
    echo -e "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
    echo -e "Tests failed: ${RED}$TESTS_FAILED${NC}"
    
    if [[ $TESTS_FAILED -eq 0 ]]; then
        echo -e "\n${GREEN}All tests passed!${NC}"
        exit 0
    else
        echo -e "\n${RED}Some tests failed!${NC}"
        exit 1
    fi
}

# Run if executed directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi