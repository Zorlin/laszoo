# Jetpack Integration Design Document

## Overview

This document outlines the integration of Jetpack (a Rust-based automation tool) into Laszoo as a metapackage system. The integration will allow Laszoo users to:

1. Store and manage Jetpack playbooks in the distributed MooseFS filesystem
2. Automatically maintain Jetpack-compatible inventory files based on Laszoo groups
3. Run playbooks using a new `++playbook` metapackage syntax
4. Execute playbooks locally on each machine or against remote hosts

## Architecture

### 1. Playbook Storage System

**Default Storage Location**: `/mnt/laszoo/playbooks/`

Playbooks will be organized in a hierarchical structure:
```
/mnt/laszoo/
├── playbooks/
│   ├── moosefs-playbook/
│   │   ├── playbook.yml
│   │   └── roles/
│   ├── webserver-playbook/
│   │   └── playbook.yml
│   └── database-playbook/
│       └── playbook.yml
└── inventory/
    └── jetpack/
        ├── groups/
        │   ├── metaloggers.yml
        │   ├── chunkservers.yml
        │   └── masters.yml
        └── host_vars/
            ├── host1.yml
            └── host2.yml
```

### 2. Inventory Synchronization

Laszoo will automatically generate and maintain Jetpack-compatible inventory files based on its group membership:

- **Source**: Laszoo's `GroupManifest` and per-machine `groups.conf`
- **Target**: `/mnt/laszoo/inventory/jetpack/groups/*.yml`
- **Format**: YAML files matching Jetpack's static inventory format
- **Update Frequency**: On group membership changes and during watch cycles

### 3. Metapackage Integration

The `++playbook` metapackage will be added to the existing metapackage system in `packages.conf`:

```
# Example packages.conf entry
++playbook moosefs-playbook
++playbook /mnt/laszoo/playbooks/custom-playbook --inventory /custom/inventory
++playbook ./local-playbook.yml
++playbook /absolute/path/to/playbook.yml
```

### 4. Command Line Interface

New subcommands will be added to the Laszoo CLI:

```bash
# Add a playbook to the distributed filesystem
laszoo playbook add /path/to/local/playbook/
laszoo playbook add /path/to/local/playbook/ --path custom-name
laszoo playbook add /path/to/local/playbook/ -p different-location

# List available playbooks
laszoo playbook list

# Run a playbook
laszoo playbook run moosefs-playbook
laszoo playbook run ./relative/path/playbook.yml
laszoo playbook run /absolute/path/playbook.yml

# Remove a playbook
laszoo playbook remove moosefs-playbook
```

## Implementation Plan

### Phase 1: Core Infrastructure

1. **Add Jetpack dependency** to `Cargo.toml`
2. **Create playbook module** (`src/playbook/mod.rs`) with:
   - `PlaybookManager` struct for managing playbooks
   - `PlaybookManifest` for tracking stored playbooks
   - Functions for adding, listing, and removing playbooks

### Phase 2: Inventory Generation

1. **Create inventory module** (`src/inventory/mod.rs`) with:
   - `InventoryGenerator` struct
   - Function to convert Laszoo groups to Jetpack inventory format
   - Automatic synchronization during watch cycles

### Phase 3: Metapackage Support

1. **Extend `PackageOperation` enum** to include `RunPlaybook` variant
2. **Update `write_packages_conf`** to serialize playbook operations
3. **Implement playbook execution** in package application logic

### Phase 4: CLI Integration

1. **Add `playbook` subcommand** to CLI parser
2. **Implement command handlers** for add, list, run, and remove
3. **Add path resolution logic** for playbook references

### Phase 5: Configuration

1. **Add configuration options**:
   - `playbook_storage_path` (default: `/mnt/laszoo/playbooks/`)
   - `playbook_precedence` (default: laszoo folder wins)
   - `inventory_sync_enabled` (default: true)
   - `jetpack_ssh_key_path` (default: `~/.ssh/id_ed25519`)

## Path Resolution Algorithm

When running `laszoo playbook run <name>`:

1. If `<name>` starts with `/` → treat as absolute path
2. If `<name>` starts with `./` or `../` → treat as relative path
3. Otherwise, check in order (configurable precedence):
   - `/mnt/laszoo/playbooks/<name>/playbook.yml`
   - `/mnt/laszoo/playbooks/<name>.yml`
   - Current directory for `<name>`

## Security Considerations

1. **SSH Key Management**: Users must manually distribute their ed25519 public keys for remote execution
2. **Playbook Validation**: Basic syntax validation before storage
3. **Permissions**: Respect MooseFS permissions for playbook access
4. **Audit Trail**: Log all playbook executions in Laszoo's git repository

## Integration with Existing Features

1. **Watch Mode**: Detect changes to `++playbook` entries and execute automatically
2. **Git Integration**: Commit playbook additions/removals to the Laszoo git repository
3. **Group Templates**: Allow embedding playbook references in group configurations
4. **Compliance**: Track playbook execution status for compliance reporting

## Future Enhancements

1. **Playbook Templates**: Support for Handlebars/quack tags in playbooks
2. **Conditional Execution**: Run playbooks based on facts or conditions
3. **Playbook Dependencies**: Define playbook execution order
4. **Remote Execution UI**: Web interface for running playbooks across infrastructure
5. **Integration with triggers**: Run playbooks as before/after hooks for other operations

## Technical Details

### Jetpack API Usage

```rust
use jetpack::{PlaybookRunner, ConnectionMode, run_playbook};

// Example of running a playbook programmatically
let result = run_playbook(playbook_path)
    .inventory(inventory_path)
    .local() // or .ssh() for remote execution
    .threads(4)
    .run()?;
```

### Inventory Format Example

```yaml
# /mnt/laszoo/inventory/jetpack/groups/metaloggers.yml
hosts:
  - mfsmetalogger1
  - mfsmetalogger2
subgroups: []
```

### Playbook Manifest Structure

```json
{
  "playbooks": {
    "moosefs-playbook": {
      "path": "/mnt/laszoo/playbooks/moosefs-playbook/",
      "description": "MooseFS cluster configuration",
      "added_by": "user@host",
      "added_at": "2024-01-20T10:00:00Z",
      "last_run": "2024-01-20T11:00:00Z",
      "run_count": 5
    }
  }
}
```

## Testing Strategy

1. **Unit Tests**: Test individual components (inventory generation, path resolution)
2. **Integration Tests**: Test full workflow with real MooseFS mount
3. **Multi-machine Tests**: Verify inventory sync and playbook execution across cluster
4. **Edge Cases**: Test path precedence, missing playbooks, invalid syntax

## Migration Path

For users with existing automation:
1. Provide import tool for converting from other automation formats
2. Document Jetpack module syntax and best practices
3. Gradual migration using both systems in parallel