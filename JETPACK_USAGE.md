# Jetpack Integration Usage Guide

## Jetpack Module Syntax

Jetpack/Jetporch uses YAML tags (!) to denote different modules:

- `!shell` - Execute shell commands
- `!echo` - Print messages
- `!file` - Manage files
- `!directory` - Manage directories
- `!template` - Apply templates
- `!git` - Git operations
- `!apt` - APT package management (Debian/Ubuntu)
- `!dnf` - DNF package management (RHEL/CentOS)
- `!sd_service` - SystemD service management
- `!user` - User management
- `!group` - Group management
- `!facts` - Gather system facts
- `!assert` - Assert conditions
- `!debug` - Debug variables
- `!stat` - Get file statistics
- `!copy` - Copy files
- `!external` - Run external modules
- `!fail` - Fail with message
- `!set` - Set variables

## Quick Start

### 1. Add a Playbook
```bash
# Add a local playbook to Laszoo's distributed storage
laszoo playbook add ~/my-playbooks/webserver-setup/

# Add with custom name
laszoo playbook add ~/my-playbooks/nginx/ --name nginx-config

# Add to custom path in MooseFS
laszoo playbook add ./deploy.yml --path infrastructure/deployments
```

### 2. List Available Playbooks
```bash
laszoo playbook list
```

### 3. Run a Playbook
```bash
# Run by name (from storage)
laszoo playbook run nginx-config

# Run with custom inventory
laszoo playbook run webserver-setup --inventory /custom/inventory/

# Run local playbook
laszoo playbook run ./local-playbook.yml

# Run with additional args
laszoo playbook run deploy -- --check --diff
```

### 4. Use in packages.conf
```bash
# Add to a group's packages.conf
echo "++playbook nginx-config" >> /mnt/laszoo/groups/webservers/etc/laszoo/packages.conf

# Machines in the group will run this playbook during watch mode
laszoo watch --auto
```

## Example Workflows

### Deploy Web Application
```bash
# 1. Create playbook locally
cat > deploy-app.yml << 'EOF'
- name: Deploy web application
  groups:
    - all
  
  sudo: root
  
  tasks:
    - !git
      name: Pull latest code
      repo: https://github.com/zorlin/laszoo
      path: /opt/laszoo
      branch: main
    
    - !shell
      name: Build Laszoo
      cmd: "cd /opt/laszoo && cargo build --release"
    
    - !sd_service
      name: laszoo
      state: restarted
EOF

# 2. Add to Laszoo
laszoo playbook add deploy-app.yml --name deploy-laszoo

# 3. Apply to web servers group
echo "++playbook deploy-laszoo" >> /mnt/laszoo/groups/webservers/etc/laszoo/packages.conf

# 4. Watch will auto-deploy
laszoo watch --auto
```

### Configure MooseFS Cluster
```bash
# 1. Create MooseFS playbook directory
mkdir -p moosefs-playbook/roles/common/tasks
mkdir -p moosefs-playbook/roles/master/tasks
mkdir -p moosefs-playbook/roles/chunkserver/tasks

# 2. Main playbook
cat > moosefs-playbook/playbook.yml << 'EOF'
- name: Configure MooseFS cluster
  groups:
    - all
  
  roles:
    - common

- name: Configure MooseFS master
  groups:
    - masters
  
  roles:
    - master

- name: Configure MooseFS chunkservers
  groups:
    - chunkservers
  
  roles:
    - chunkserver
EOF

# 3. Add to Laszoo
laszoo playbook add moosefs-playbook/

# 4. Run on all MooseFS nodes
laszoo playbook run moosefs-playbook
```

### System Updates with Pre/Post Hooks
```bash
# Create update playbook with safety checks
cat > safe-update.yml << 'EOF'
- name: Safe system update
  groups:
    - all
  
  serial: 1  # One host at a time
  sudo: root
  
  tasks:
    - !shell
      name: Check service health
      cmd: "curl -f http://localhost/health"
      save: health_check
    
    - !shell
      name: Create checkpoint
      cmd: 'laszoo commit -m "Pre-update checkpoint"'
    
    - !apt
      name: Update all packages
      package: "*"
      state: latest
      with:
        condition: (eq jet_os_flavor "Debian")
    
    - !dnf
      name: Update all packages
      package: "*"
      state: latest
      with:
        condition: (eq jet_os_flavor "EL")
    
    - !sd_service
      name: nginx
      state: started
    
    - !sd_service
      name: laszoo
      state: started
EOF

laszoo playbook add safe-update.yml
echo "++playbook safe-update" >> /mnt/laszoo/groups/production/etc/laszoo/packages.conf
```

## Inventory Management

### Automatic Inventory
Laszoo automatically generates Jetpack-compatible inventory from its groups:

```yaml
# /mnt/laszoo/inventory/jetpack/groups/webservers.yml
hosts:
  - web1
  - web2
  - web3
subgroups: []

# /mnt/laszoo/inventory/jetpack/host_vars/web1.yml
host: web1
laszoo_managed: true
```

### Custom Host Variables
Add custom variables for specific hosts:
```bash
# Edit host vars
vi /mnt/laszoo/inventory/jetpack/host_vars/web1.yml

# Add custom variables
cat >> /mnt/laszoo/inventory/jetpack/host_vars/web1.yml << 'EOF'
http_port: 8080
ssl_enabled: true
environment: production
EOF
```

## Integration with Laszoo Features

### Combine with Templates
```bash
# 1. Enroll nginx config as template
laszoo enroll webservers /etc/nginx/nginx.conf

# 2. Edit template with variables
vi /mnt/laszoo/groups/webservers/etc/nginx/nginx.conf.lasz

# 3. Create playbook to reload after changes
cat > nginx-reload.yml << 'EOF'
- name: Reload nginx after config change
  groups:
    - all
  
  sudo: root
  
  tasks:
    - !shell
      name: Test nginx config
      cmd: nginx -t
    
    - !sd_service
      name: nginx
      state: reloaded
EOF

# 4. Add both to group
echo "++playbook nginx-reload" >> /mnt/laszoo/groups/webservers/etc/laszoo/packages.conf
```

### Rolling Updates
```bash
# Playbook with serial execution
cat > rolling-update.yml << 'EOF'
- name: Rolling update
  groups:
    - all
  
  serial: "30%"  # Update 30% of hosts at a time
  max_fail_percentage: 20
  sudo: root
  
  tasks:
    - !shell
      name: Remove from load balancer
      cmd: "curl -X POST http://lb.internal/remove/{{ jet_hostname }}"
    
    - !shell
      name: Update application
      cmd: "cd /opt/laszoo && git pull && cargo build --release"
    
    - !shell
      name: Add back to load balancer
      cmd: "curl -X POST http://lb.internal/add/{{ jet_hostname }}"
EOF
```

## Advanced Usage

### Conditional Playbooks
```bash
# Run playbook only on specific OS
cat > ubuntu-setup.yml << 'EOF'
- name: Ubuntu-specific setup
  groups:
    - all
  
  tasks:
    - !facts
    
    - !apt
      name: Configure Ubuntu
      package: ubuntu-advantage-tools
      with:
        condition: (eq jet_os_name "Ubuntu")
EOF
```

### Dynamic Inventory Script
```bash
# Create custom dynamic inventory
laszoo inventory create-dynamic-script

# Use with external Jetpack
jetpack -i /mnt/laszoo/inventory/jetpack/dynamic_inventory.py playbook.yml
```

### Playbook Dependencies
```bash
# Main playbook that includes others
cat > site.yml << 'EOF'
# Jetpack/Jetporch uses role inclusion instead of playbook imports
- name: Complete site configuration
  groups:
    - all
  
  roles:
    - common
    - security
    - monitoring
    - app-specific
EOF
```

## Troubleshooting

### Debug Playbook Execution
```bash
# Run with verbose output
laszoo playbook run myplaybook -- -vvv

# Check playbook syntax
cd /mnt/laszoo/playbooks/myplaybook
jetpack playbook.yml --syntax-check
```

### Inventory Issues
```bash
# Force inventory sync
laszoo status  # This triggers inventory sync

# Check generated inventory
ls -la /mnt/laszoo/inventory/jetpack/groups/
cat /mnt/laszoo/inventory/jetpack/groups/mygroup.yml
```

### Watch Mode Not Running Playbooks
```bash
# Check packages.conf syntax
cat /mnt/laszoo/groups/mygroup/etc/laszoo/packages.conf

# Check watch logs
laszoo watch --auto  # Run in foreground to see output

# Verify group membership
cat /mnt/laszoo/machines/$(hostname)/etc/laszoo/groups.conf
```

## Best Practices

1. **Test First**: Always test playbooks locally before adding to Laszoo
   ```bash
   jetpack playbook.yml --check --diff
   ```

2. **Use Version Control**: Store playbook sources in git
   ```bash
   cd ~/my-playbooks
   git init
   git add .
   git commit -m "Initial playbooks"
   ```

3. **Idempotent Tasks**: Ensure playbooks can run multiple times safely

4. **Error Handling**: Add proper error handling
   ```yaml
   - !shell
     name: Critical task
     cmd: /important/script
     save: result
     failed_when: (ne result.rc 0)
     with:
       retry: 3
       retry_delay: 5
   ```

5. **Documentation**: Document playbook purpose and requirements
   ```yaml
   # This playbook configures web servers
   # Requirements:
   #   - Ubuntu 22.04 or RHEL 8+
   #   - Network access to package repos
   # Note: Jetpack playbooks use module tags (!echo, !shell, etc)
   
   - name: Web server configuration
     groups:
       - webservers
   ```

6. **Separate Concerns**: Use different playbooks for different purposes
   - `setup.yml` - Initial server setup
   - `deploy.yml` - Application deployment
   - `maintain.yml` - Routine maintenance
   - `emergency.yml` - Emergency procedures

## Security Considerations

1. **SSH Keys**: For remote execution, distribute keys securely
   ```bash
   # Generate if needed
   ssh-keygen -t ed25519 -f ~/.ssh/id_ed25519
   
   # Distribute to target hosts
   ssh-copy-id -i ~/.ssh/id_ed25519.pub user@target
   ```

2. **Secrets Management**: Don't store secrets in playbooks
   ```yaml
   # Bad
   - !user
     name: admin
     password: "plaintext123"
   
   # Good
   - !user
     name: admin
     password: "{{ admin_password_hash }}"
   ```

3. **Audit Trail**: All playbook executions are logged
   ```bash
   # Check action history
   ls -la /mnt/laszoo/actions/$(hostname)/
   ```

## Future Enhancements

- Playbook templates with Laszoo variables
- Conditional execution based on facts
- Integration with Laszoo triggers
- Web UI for playbook management
- Approval workflow for production playbooks
- Integration with Laszoo's secret management