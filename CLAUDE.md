# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# Laszoo - Distributed Configuration Management

Laszoo is a distributed configuration management tool that leverages MooseFS for zero-config clustering and automatic synchronization of configuration files across hosts.

## Design Principles

### Stateless Architecture
Laszoo follows a stateless design where the filesystem structure represents the system state. This means:

- Group membership is expressed through directories containing symlinks, not through YAML files listing hosts
- Templates are stored as individual `.lasz` files in their respective directories
- Relationships between entities use filesystem structure (directories and symlinks)
- Operations are idempotent - running them multiple times produces the same result

### When YAML files are appropriate
YAML files are used for configuration data, not state tracking:
- `group_vars/desktop` - Variables for the desktop group
- `host_vars/green` - Variables specific to the green host
- `/etc/laszoo/packages.conf` - Package configuration
- These files contain configuration values, not lists of relationships or state

### Filesystem conventions
- Use timestamps in filenames when ordering matters
- Express relationships through directory structure
- Let the filesystem be the source of truth for system state

## Development Commands

### Building
```bash
cargo build           # Debug build
cargo build --release # Release build (required for production use)
```

### Running
```bash
./target/release/laszoo <command>  # Run from build directory
sudo ./target/release/laszoo <command>  # Some operations require root
```

### Common Development Tasks
```bash
# Watch for changes and auto-apply templates
./target/release/laszoo watch -a --hard

# Check status of enrolled files
./target/release/laszoo status

# Enroll a file or directory
./target/release/laszoo enroll <group> <path>

# Apply templates manually
./target/release/laszoo apply <group>
```

## Architecture Overview

### Core Modules

1. **Enrollment Manager** (`src/enrollment/mod.rs`)
   - Handles file/directory enrollment into groups
   - Manages machine and group manifests
   - Creates and applies templates
   - Key types: `EnrollmentManager`, `EnrollmentManifest`, `EnrollmentEntry`

2. **Template Engine** (`src/template/mod.rs`)
   - Processes Handlebars variables (`{{ variable }}`)
   - Handles quack tags (`[[x content x]]`) for machine-specific content
   - Supports hybrid mode where group templates can include machine-specific sections

3. **Sync Engine** (`src/sync/mod.rs`)
   - Implements synchronization strategies (converge, rollback, forward, freeze, drift)
   - Handles conflict resolution between local changes and templates
   - Manages file version tracking across machines

4. **Git Integration** (`src/git/mod.rs`)
   - Automatically commits template changes
   - Uses Ollama for AI-generated commit messages
   - Falls back to generic commit messages when Ollama unavailable
   - Tracks changes in `/mnt/laszoo/.git/`

5. **Watch Mode** (`src/main.rs` - `watch_files` function)
   - Uses inotify for local file monitoring
   - Polls MooseFS templates every 2 seconds (no inotify support)
   - Detects local vs remote changes using checksums
   - Auto-applies remote template changes when enabled

### Data Flow

1. **File Enrollment**:
   - Local file → Create template in `/mnt/laszoo/groups/<group>/<path>.lasz`
   - Update manifests (group and/or machine specific)
   - Other machines detect new template and apply

2. **Change Detection**:
   - Local changes: inotify → update template → git commit
   - Remote changes: checksum comparison → apply template locally
   - Uses SHA-256 checksums for reliable change detection

3. **Template Application**:
   - Read template → Process Handlebars/quack tags → Write to local file
   - Preserve file permissions (but not timestamps)
   - Skip manifest entries for files within enrolled directories

### Key Design Decisions

1. **MooseFS Dependency**: All coordination happens through shared filesystem
2. **No Central Server**: Machines coordinate through files in `/mnt/laszoo/`
3. **Git for History**: All changes tracked in git repository at mount point
4. **Checksum-based Detection**: More reliable than timestamp comparison
5. **Background Operations**: Git commits run async to avoid blocking

### Important Paths

- **MooseFS Mount**: `/mnt/laszoo/` (configurable)
- **Group Templates**: `/mnt/laszoo/groups/<group>/<file-path>.lasz`
- **Machine Templates**: `/mnt/laszoo/machines/<hostname>/<file-path>.lasz`
- **Manifests**: `manifest.json` in group/machine directories
- **Git Repository**: `/mnt/laszoo/.git/`

### Inventory Structure (Jetpack)

Jetpack uses its own inventory format:

```
/mnt/laszoo/inventory/jetpack/
├── groups/
│   ├── all           # Group file with hosts: and subgroups: lists
│   ├── desktop       # Example: hosts: [green, blackberry]
│   └── proxmox       # Example: hosts: [blue], subgroups: []
├── group_vars/
│   ├── all           # Variables for all group
│   ├── desktop       # Variables for desktop group
│   └── proxmox       # Variables for proxmox group
└── host_vars/
    ├── green         # Host-specific variables
    └── blue          # Host-specific variables
```

Note: Jetpack's group files follow a specific format with `hosts:` and `subgroups:` keys. Group files, group_vars, and host_vars do NOT use .yml extensions.

### Critical Implementation Details

1. **Template Change Detection**: Uses checksums, not timestamps (see `calculate_file_checksum`)
2. **Circular Update Prevention**: Tracks local changes to avoid re-applying own updates
3. **Directory Enrollment**: Files within enrolled directories are adopted, not individually tracked
4. **Race Condition Handling**: Channel-based tracking of in-flight git commits
5. **Ignore Mechanism**: Temporary ignore list prevents processing loops during template application

### Common Issues and Solutions

1. **Infinite Loop**: Fixed by ignore mechanism with 5-second timeout
2. **Missing Change Detection**: Fixed by using checksums instead of timestamps
3. **Incorrect Manifest Entries**: Fixed by checking enrolled directories before creating entries
4. **Git Commit Races**: Fixed by tracking pending commits with channels
5. **Slow Sync**: Reduced template scan interval from 10s to 2s

## Testing Considerations

- No unit tests currently exist
- Manual testing involves multiple machines sharing MooseFS mount
- Test scenarios should include concurrent edits, network partitions, and git conflicts
- Always test with release builds as debug builds may have different timing
- **NEVER use mocks in tests** - all tests must use real file operations, real MooseFS mounts, and real git operations

## Future Work (from TODO list)

- Implement merge_file_changes_to_template for converge mode
- Add trigger execution when applying templates
- Fix machine templates to maintain directory structure
- Implement diff command
- Add action management (--before/--after triggers)
- Implement package management system
- Add compliance reporting

# important-instruction-reminders
Do what has been asked; nothing more, nothing less.
NEVER create files unless they're absolutely necessary for achieving your goal.
ALWAYS prefer editing an existing file to creating a new one.
NEVER proactively create documentation files (*.md) or README files. Only create documentation files if explicitly requested by the User.
NEVER MARK TASKS AS COMPLETED IN TODO UNTIL THEY HAVE BEEN TESTED AND VERIFIED TO WORK. Building without errors IS NOT THE SAME AS WORKING.