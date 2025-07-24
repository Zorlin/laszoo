# Laszoo - Distributed Configuration Management

Laszoo is a distributed configuration management tool designed to work with distributed filesystems like MooseFS or CephFS. It provides file enrollment, templating, synchronization, and version control capabilities across multiple hosts.

## Features

- **File Enrollment**: Track and manage configuration files across hosts
- **Template Engine**: Handlebars-based templating with custom `[[x content x]]` quack tags
- **Multi-Host Synchronization**: Automatic rollback/forward strategies for configuration consistency
- **Change Detection**: Monitor enrolled files for modifications
- **Git Integration**: AI-powered commit messages using Ollama
- **Group Management**: Organize hosts into logical groups for targeted configuration management

## Installation

```bash
cargo build --release
```

## Usage

### Initialize Laszoo

```bash
laszoo init --mfs-mount /mnt/mfs
```

### Enroll Files

```bash
laszoo enroll webserver /etc/nginx/nginx.conf /etc/apache2/apache2.conf
```

### Create Groups

```bash
laszoo group create production --description "Production servers"
laszoo group add-host production --host server1
```

### Synchronize Files

```bash
laszoo sync --group webserver --strategy auto
```

### Apply Templates

```bash
laszoo apply template.conf.lasz
```

### Commit Changes

```bash
laszoo commit --all --message "Updated web server configuration"
```

## Configuration

Configuration is stored in `~/.laszoo/config.toml`:

```toml
mfs_mount = "/mnt/mfs"
laszoo_dir = "laszoo"
default_sync_strategy = "auto"
auto_commit = true
ollama_endpoint = "http://localhost:11434"
ollama_model = "qwen3:14b"

[monitoring]
enabled = true
debounce_ms = 500
poll_interval = 30

[logging]
level = "info"
format = "pretty"
```

## Architecture

Laszoo stores all configuration data in the distributed filesystem under `{mfs_mount}/laszoo/`:

- `{hostname}/manifest.json` - Enrolled files for each host
- `{hostname}/{path}/*.lasz` - Template versions of enrolled files
- `groups.json` - Group definitions and memberships

## Requirements

- MooseFS or CephFS mounted filesystem
- Ollama (for AI-powered commit messages)
- Git (for version control features)

## License

This project is part of the Laszoo distributed configuration management system.