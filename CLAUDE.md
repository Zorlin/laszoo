# Laszoo - Distributed Configuration Management

Laszoo is a distributed configuration management tool that leverages MooseFS for zero-config clustering and automatic synchronization of configuration files across hosts.

## Overview

Laszoo manages configuration files across multiple hosts by:
- Storing configuration templates in MooseFS at `/mnt/mfs/laszoo/`
- Creating `.lasz` template files when hosts diverge
- Supporting basic Handlebars templating with `{{ variable }}` syntax
- Allowing intentional divergence with quack! tags `[[x content x]]`
- Automatically detecting and handling configuration drift
- Committing changes to Git with AI-generated summaries

## Architecture

Each host runs a Laszoo instance that:
1. Monitors enrolled configuration files
2. Stores templates in MooseFS at `/mnt/mfs/laszoo/<hostname>/<path-to-file>.lasz`
3. Symlinks original files to their MooseFS-backed templates
4. Synchronizes changes across hosts in the same group
5. Maintains a local Git repository for change tracking

## Key Features

### File Enrollment
```bash
laszoo enroll <group> <file-or-directory>
```
Enrolls configuration files into a management group. Creates `.lasz` templates only when hosts diverge.

### Templating
- **Handlebars**: `{{ variable }}` - Variables replaced when applying configurations
- **Quack! Tags**: `[[x content x]]` - Renders as "content" but allows intentional per-host divergence

### Synchronization Strategies
- **Rollback**: Reverts changes if majority of nodes have different configuration
- **Forward**: Propagates local changes to all nodes in the group

### Zero-Config Clustering
All Laszoo instances sharing the same MooseFS mountpoint automatically form a cluster without additional configuration.

## Implementation Notes

- Language: Rust
- License: AGPLv3
- Dependencies: MooseFS client (assumed pre-installed at `/mnt/mfs`)
- Git integration: Uses Ollama for commit message generation
- Jetpack integration: Future feature (not implemented in initial version)

## Technical Details

### File Management
- Original config: `/etc/service/config.conf`
- Template (if diverged): `/etc/service/config.conf.lasz`
- MooseFS storage: `/mnt/mfs/laszoo/<hostname>/etc/service/config.conf.lasz`

### Change Detection
Laszoo monitors both the original file and template, detecting:
- Manual edits to service configurations
- Template modifications
- Divergence from group consensus

### Failure Modes
When MooseFS is unavailable:
- Laszoo stops making modifications
- Existing configurations remain unchanged
- Service continues with last known configuration

## Future Enterprise Features
- Web-based management GUI
- Drag-and-drop group management
- Visual diff and merge tools
- Business Source License with 2-year AGPLv3 conversion