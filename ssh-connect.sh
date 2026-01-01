#!/bin/bash
#===============================================================================
# SSH Connection Script
# Version: 1.0.1
# Description: Connect to remote server using SSH key authentication
#===============================================================================

VERSION="1.0.1"
SSH_KEY="/Users/sadeghhp/Downloads/vpn11_key.pem"
REMOTE_USER="sadeghhp"
REMOTE_HOST="4.246.219.176"
SSH_PORT="22"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  SSH Connection Script v${VERSION}${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Safety check: Verify SSH key exists
if [[ ! -f "$SSH_KEY" ]]; then
    echo -e "${RED}ERROR: SSH key not found at: ${SSH_KEY}${NC}"
    exit 1
fi

# Safety check: Verify SSH key permissions
KEY_PERMS=$(stat -f "%Lp" "$SSH_KEY" 2>/dev/null || stat -c "%a" "$SSH_KEY" 2>/dev/null)
if [[ "$KEY_PERMS" != "600" && "$KEY_PERMS" != "400" ]]; then
    echo -e "${YELLOW}WARNING: SSH key permissions are ${KEY_PERMS}, fixing to 600...${NC}"
    chmod 600 "$SSH_KEY"
    echo -e "${GREEN}Permissions fixed.${NC}"
fi

# Display connection info
echo -e "${GREEN}Connection Details:${NC}"
echo -e "  Host:     ${REMOTE_HOST}"
echo -e "  Port:     ${SSH_PORT}"
echo -e "  User:     ${REMOTE_USER}"
echo -e "  Key:      ${SSH_KEY}"
echo ""

# Interactive confirmation
read -p "Do you want to connect? (y/n): " -n 1 -r
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}Connection cancelled.${NC}"
    exit 0
fi

echo -e "${GREEN}Connecting...${NC}"
echo ""

# Execute SSH connection
ssh -i "$SSH_KEY" \
    -o StrictHostKeyChecking=no \
    -o ServerAliveInterval=60 \
    -o ServerAliveCountMax=3 \
    -p "$SSH_PORT" \
    "${REMOTE_USER}@${REMOTE_HOST}"

# Capture exit status
EXIT_STATUS=$?

echo ""
if [[ $EXIT_STATUS -eq 0 ]]; then
    echo -e "${GREEN}SSH session ended successfully.${NC}"
else
    echo -e "${RED}SSH connection failed with exit code: ${EXIT_STATUS}${NC}"
fi

exit $EXIT_STATUS

