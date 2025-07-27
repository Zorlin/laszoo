#!/bin/bash
# Setup script for Jetpack integration directories
# This script creates the necessary directories with proper permissions

set -e

MFS_MOUNT="${MFS_MOUNT:-/mnt/laszoo}"

echo "Setting up Jetpack integration directories in $MFS_MOUNT..."

# Create playbooks directory
if [ ! -d "$MFS_MOUNT/playbooks" ]; then
    echo "Creating playbooks directory..."
    sudo mkdir -p "$MFS_MOUNT/playbooks"
    sudo chmod 755 "$MFS_MOUNT/playbooks"
fi

# Create inventory directories
if [ ! -d "$MFS_MOUNT/inventory/jetpack" ]; then
    echo "Creating inventory directories..."
    sudo mkdir -p "$MFS_MOUNT/inventory/jetpack/groups"
    sudo mkdir -p "$MFS_MOUNT/inventory/jetpack/host_vars"
    sudo chmod -R 755 "$MFS_MOUNT/inventory"
fi

# Create testing directory for custom paths (optional)
if [ ! -d "$MFS_MOUNT/testing" ]; then
    echo "Creating testing directory..."
    sudo mkdir -p "$MFS_MOUNT/testing"
    sudo chmod 755 "$MFS_MOUNT/testing"
fi

echo "Jetpack directories setup complete!"
echo ""
echo "Directory structure:"
echo "  $MFS_MOUNT/playbooks/         - Playbook storage"
echo "  $MFS_MOUNT/inventory/jetpack/ - Inventory files"
echo "  $MFS_MOUNT/testing/           - Custom path testing"