#!/bin/bash
#===============================================================================
# MyTunnel Deployment Script
# Version: 1.0.0
# Description: Build and deploy MyTunnel to a remote server as a systemd service
#===============================================================================

VERSION="1.0.0"

# Source connection details from ssh-connect.sh if it exists
if [[ -f "ssh-connect.sh" ]]; then
    # Extract variables without executing the whole script (which is interactive)
    REMOTE_USER=$(grep "REMOTE_USER=" ssh-connect.sh | cut -d'"' -f2)
    REMOTE_HOST=$(grep "REMOTE_HOST=" ssh-connect.sh | cut -d'"' -f2)
    SSH_PORT=$(grep "SSH_PORT=" ssh-connect.sh | cut -d'"' -f2)
    SSH_KEY=$(grep "SSH_KEY=" ssh-connect.sh | cut -d'"' -f2)
else
    echo "Error: ssh-connect.sh not found. Please ensure it exists with server details."
    exit 1
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  MyTunnel Publish Script v${VERSION}${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Display target info
echo -e "${GREEN}Target Server:${NC}"
echo -e "  Host: ${REMOTE_HOST}"
echo -e "  User: ${REMOTE_USER}"
echo ""

# Safety check: Verify SSH key
if [[ ! -f "$SSH_KEY" ]]; then
    echo -e "${RED}ERROR: SSH key not found at: ${SSH_KEY}${NC}"
    exit 1
fi

# Ask for confirmation
read -p "Proceed with deployment to ${REMOTE_HOST}? (y/n): " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}Deployment cancelled.${NC}"
    exit 0
fi

# Step 1: Sync source code to server
echo -e "${BLUE}[1/4] Syncing source code to server...${NC}"
rsync -avz -e "ssh -i $SSH_KEY -p $SSH_PORT -o StrictHostKeyChecking=no" \
    --exclude 'target' \
    --exclude '.git' \
    --exclude '.cursor' \
    ./ "${REMOTE_USER}@${REMOTE_HOST}:~/mytunnel-src"

if [[ $? -ne 0 ]]; then
    echo -e "${RED}Failed to sync source code.${NC}"
    exit 1
fi

# Step 2: Build and Install on server
echo -e "${BLUE}[2/4] Building and installing on server...${NC}"
ssh -i "$SSH_KEY" -p "$SSH_PORT" -o StrictHostKeyChecking=no "${REMOTE_USER}@${REMOTE_HOST}" << 'EOF'
    set -e
    echo "Checking for Rust/Cargo..."
    if ! command -v cargo &> /dev/null; then
        echo "Installing Rust..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source $HOME/.cargo/env
    fi

    cd ~/mytunnel-src
    echo "Building release binary..."
    cargo build --release

    echo "Installing binary..."
    sudo cp target/release/mytunnel-server /usr/local/bin/
    sudo chmod +x /usr/local/bin/mytunnel-server

    echo "Setting up configuration directory..."
    sudo mkdir -p /etc/mytunnel
    if [[ ! -f "/etc/mytunnel/config.toml" ]]; then
        sudo cp config.example.toml /etc/mytunnel/config.toml
        echo "Default configuration created at /etc/mytunnel/config.toml"
    fi
EOF

if [[ $? -ne 0 ]]; then
    echo -e "${RED}Failed to build or install on server.${NC}"
    exit 1
fi

# Step 3: Setup Systemd Service
echo -e "${BLUE}[3/4] Setting up systemd service...${NC}"
scp -i "$SSH_KEY" -P "$SSH_PORT" -o StrictHostKeyChecking=no mytunnel.service "${REMOTE_USER}@${REMOTE_HOST}:/tmp/mytunnel.service"

ssh -i "$SSH_KEY" -p "$SSH_PORT" -o StrictHostKeyChecking=no "${REMOTE_USER}@${REMOTE_HOST}" << 'EOF'
    set -e
    sudo mv /tmp/mytunnel.service /etc/systemd/system/
    sudo systemctl daemon-reload
    sudo systemctl enable mytunnel
    echo "Service configured and enabled."
EOF

# Step 4: Start/Restart Service
echo -e "${BLUE}[4/4] Starting/Restarting service...${NC}"
ssh -i "$SSH_KEY" -p "$SSH_PORT" -o StrictHostKeyChecking=no "${REMOTE_USER}@${REMOTE_HOST}" << 'EOF'
    sudo systemctl restart mytunnel
    echo "Service status:"
    sudo systemctl is-active mytunnel || echo "Service failed to start (expected if certs are missing)"
    echo "Check logs with: journalctl -u mytunnel -f"
EOF

echo ""
echo -e "${GREEN}Deployment completed successfully!${NC}"
echo -e "You may need to configure TLS certificates in /etc/mytunnel/config.toml"



