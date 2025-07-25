# Laszoo Setup and Usage Instructions

## Prerequisites

1. **MooseFS or CephFS**: Must be installed and mounted
   ```bash
   # Verify MooseFS is mounted
   mount | grep mfs
   # Should show: mfsmount on /mnt/mfs type fuse.mfsmount ...
   ```

2. **Ollama**: Required for AI-powered commit messages
   ```bash
   # Install Ollama
   curl -fsSL https://ollama.ai/install.sh | sh
   
   # Start Ollama service
   ollama serve
   
   # Pull a model (e.g., qwen3:14b)
   ollama pull qwen3:14b
   ```

3. **Git**: For version control features
   ```bash
   # Configure git user
   git config --global user.name "Your Name"
   git config --global user.email "your.email@example.com"
   ```

## Installation

```bash
# Clone the repository
git clone <repository-url>
cd laszoo

# Build the project
cargo build --release

# Copy binary to PATH (optional)
sudo cp target/release/laszoo /usr/local/bin/
```

## Initial Setup

1. **Initialize Laszoo** (one-time setup per host)
   ```bash
   laszoo init --mfs-mount /mnt/mfs
   ```

2. **Create a configuration file** (optional)
   ```bash
   mkdir -p ~/.laszoo
   cat > ~/.laszoo/config.toml << EOF
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
   EOF
   ```

## Basic Workflow

### 1. Create Groups

Groups organize hosts for targeted configuration management:

```bash
# Create groups
laszoo group create webservers --description "Web server hosts"
laszoo group create databases --description "Database servers"
laszoo group create development --description "Development machines"

# Add hosts to groups
laszoo group add-host webservers --host web01
laszoo group add-host webservers --host web02
laszoo group add-host databases --host db01

# Add current host to a group
laszoo group add-host development

# List all groups
laszoo group list
```

### 2. Enroll Configuration Files

Enroll files to be managed by Laszoo:

```bash
# Enroll single file
laszoo enroll webservers /etc/nginx/nginx.conf

# Enroll multiple files
laszoo enroll webservers /etc/nginx/nginx.conf /etc/nginx/sites-available/default

# Enroll entire directory
laszoo enroll databases /etc/mysql/

# List enrolled files
laszoo list
```

### 3. Create and Apply Templates

Templates use Handlebars syntax with special quack tags:

```bash
# Create a template file manually
cat > /tmp/nginx.conf.lasz << 'EOF'
worker_processes {{ cpu_count }};

events {
    worker_connections 1024;
}

http {
    server {
        listen 80;
        server_name {{ hostname }};
        
        # Host-specific configuration
        [[x 
        # This content can vary per host
        location /special {
            return 200 "Host: {{ hostname }}";
        }
        x]]
    }
}
EOF

# Apply template with variables
laszoo apply /tmp/nginx.conf.lasz \
  --output /etc/nginx/nginx.conf \
  --var cpu_count=4 \
  --var hostname=web01.example.com
```

### 4. Synchronize Configurations

Sync configurations across hosts in a group:

```bash
# Auto strategy (rollback or forward based on majority)
laszoo sync --group webservers

# Force rollback to majority configuration
laszoo sync --group webservers --strategy rollback

# Force forward local changes to all hosts
laszoo sync --group webservers --strategy forward

# Dry run to see what would change
laszoo sync --group webservers --dry-run
```

### 5. Monitor Changes

Laszoo can monitor enrolled files for changes:

```bash
# Start monitoring in the background
laszoo monitor --daemon

# Check monitoring status
laszoo status
```

### 6. Version Control with Git

Commit configuration changes with AI-generated messages:

```bash
# Stage and commit all changes
laszoo commit --all

# Commit with custom context
laszoo commit --all --message "Updated SSL certificates"

# Manual git operations in Laszoo directory
cd /mnt/mfs/laszoo
git log --oneline
```

## Advanced Usage

### Working with Templates

1. **Variables in templates**:
   ```handlebars
   server {
       listen {{ port | default: 80 }};
       server_name {{ hostname }};
       root {{ document_root | default: "/var/www/html" }};
   }
   ```

2. **Quack tags for host-specific content**:
   ```handlebars
   # Common configuration
   log_level = info
   
   [[x
   # This section can differ between hosts
   # Host A might have: debug_mode = true
   # Host B might have: debug_mode = false
   x]]
   ```

### Multi-Host Scenarios

1. **Rolling updates**:
   ```bash
   # Update configuration on one host
   laszoo enroll webservers /etc/app/config.json
   
   # Test on that host
   systemctl restart app
   
   # If successful, sync to others
   laszoo sync --group webservers --strategy forward
   ```

2. **Emergency rollback**:
   ```bash
   # If a bad config was deployed
   laszoo sync --group webservers --strategy rollback
   ```

### Troubleshooting

1. **Check Laszoo status**:
   ```bash
   laszoo status
   ```

2. **View logs**:
   ```bash
   # Increase verbosity
   RUST_LOG=debug laszoo list
   
   # Check systemd logs if running as service
   journalctl -u laszoo -f
   ```

3. **Verify MooseFS connectivity**:
   ```bash
   ls -la /mnt/mfs/laszoo/
   ```

4. **Reset enrollment** (use with caution):
   ```bash
   # Remove all enrollments for current host
   rm -rf /mnt/mfs/laszoo/$(hostname)
   ```

## Best Practices

1. **Test templates before applying**:
   ```bash
   laszoo apply template.lasz --output /tmp/test.conf --dry-run
   ```

2. **Use groups effectively**:
   - Create groups based on function (webservers, databases)
   - Or by environment (production, staging, development)

3. **Commit regularly**:
   ```bash
   # After making changes
   laszoo commit --all
   ```

4. **Monitor critical files**:
   - Enroll important configuration files
   - Use file monitoring for automatic detection

5. **Document templates**:
   - Add comments explaining variables
   - Document quack tag sections

## Security Considerations

1. **File permissions**: Laszoo preserves original file permissions
2. **Access control**: Limit MooseFS mount access appropriately
3. **Sensitive data**: Use environment variables for secrets, not templates
4. **Audit trail**: All changes are tracked in Git

## Example: Complete Web Server Setup

```bash
# 1. Initialize and create group
laszoo init
laszoo group create webfarm --description "Production web servers"

# 2. Add hosts
laszoo group add-host webfarm --host web01
laszoo group add-host webfarm --host web02
laszoo group add-host webfarm --host web03

# 3. Enroll configuration files
laszoo enroll webfarm /etc/nginx/nginx.conf
laszoo enroll webfarm /etc/nginx/sites-enabled/

# 4. Create template
cat > nginx.conf.lasz << 'EOF'
worker_processes auto;
pid /run/nginx.pid;

events {
    worker_connections {{ max_connections | default: 768 }};
}

http {
    sendfile on;
    tcp_nopush on;
    types_hash_max_size 2048;
    
    include /etc/nginx/mime.types;
    default_type application/octet-stream;
    
    [[x
    # Host-specific includes or configurations
    # Can vary between web01, web02, web03
    x]]
    
    access_log /var/log/nginx/access.log;
    error_log /var/log/nginx/error.log;
    
    include /etc/nginx/conf.d/*.conf;
    include /etc/nginx/sites-enabled/*;
}
EOF

# 5. Apply template on each host
laszoo apply nginx.conf.lasz --output /etc/nginx/nginx.conf --var max_connections=1024

# 6. Sync across all hosts
laszoo sync --group webfarm --strategy forward

# 7. Commit changes
laszoo commit --all --message "Standardized nginx configuration across web farm"
```