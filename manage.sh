#!/bin/bash
#===============================================================================
# MyTunnel Management Script
# Version: 1.0.0
# Description: Build, install, and manage the MyTunnel service locally
#===============================================================================

VERSION="1.0.0"
SCRIPT_NAME=$(basename "$0")

# Paths
BINARY_NAME="mytunnel-server"
SOURCE_BINARY="target/release/${BINARY_NAME}"
INSTALL_PATH="/usr/local/bin/${BINARY_NAME}"
CONFIG_DIR="/etc/mytunnel"
CONFIG_FILE="${CONFIG_DIR}/config.toml"
SERVICE_FILE="mytunnel.service"
SYSTEMD_PATH="/etc/systemd/system/${SERVICE_FILE}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

#-------------------------------------------------------------------------------
# Helper Functions
#-------------------------------------------------------------------------------

print_header() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}  MyTunnel Manager v${VERSION}${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
}

print_usage() {
    echo -e "${CYAN}Usage:${NC} ${SCRIPT_NAME} <command>"
    echo ""
    echo -e "${CYAN}Commands:${NC}"
    echo "  build     Build the release binary"
    echo "  install   Install binary and setup systemd service"
    echo "  start     Start the mytunnel service"
    echo "  stop      Stop the mytunnel service"
    echo "  restart   Restart the mytunnel service"
    echo "  status    Show service status and information"
    echo ""
    echo -e "${CYAN}Examples:${NC}"
    echo "  ${SCRIPT_NAME} build"
    echo "  ${SCRIPT_NAME} install"
    echo "  ${SCRIPT_NAME} status"
}

check_cargo() {
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}ERROR: Cargo is not installed or not in PATH${NC}"
        echo -e "${YELLOW}Install Rust from: https://rustup.rs${NC}"
        exit 1
    fi
}

check_root() {
    if [[ $EUID -ne 0 ]]; then
        echo -e "${RED}ERROR: This operation requires root privileges${NC}"
        echo -e "${YELLOW}Please run with sudo: sudo ${SCRIPT_NAME} $1${NC}"
        exit 1
    fi
}

check_binary_exists() {
    if [[ ! -f "${SOURCE_BINARY}" ]]; then
        echo -e "${RED}ERROR: Binary not found at ${SOURCE_BINARY}${NC}"
        echo -e "${YELLOW}Run '${SCRIPT_NAME} build' first${NC}"
        exit 1
    fi
}

confirm_action() {
    local message="$1"
    read -p "${message} (y/n): " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo -e "${YELLOW}Operation cancelled.${NC}"
        exit 0
    fi
}

#-------------------------------------------------------------------------------
# Command Functions
#-------------------------------------------------------------------------------

cmd_build() {
    echo -e "${BLUE}[BUILD] Compiling release binary...${NC}"
    echo ""
    
    check_cargo
    
    echo -e "${GREEN}Cargo version:${NC} $(cargo --version)"
    echo -e "${GREEN}Building:${NC} ${BINARY_NAME}"
    echo ""
    
    cargo build --release
    
    if [[ $? -eq 0 ]]; then
        echo ""
        echo -e "${GREEN}Build successful!${NC}"
        echo -e "Binary: ${SOURCE_BINARY}"
        ls -lh "${SOURCE_BINARY}" 2>/dev/null
    else
        echo ""
        echo -e "${RED}Build failed!${NC}"
        exit 1
    fi
}

cmd_install() {
    echo -e "${BLUE}[INSTALL] Installing MyTunnel service...${NC}"
    echo ""
    
    check_root "install"
    check_binary_exists
    
    # Show what will be installed
    echo -e "${CYAN}Installation Summary:${NC}"
    echo "  Binary:  ${SOURCE_BINARY} -> ${INSTALL_PATH}"
    echo "  Config:  ${CONFIG_FILE}"
    echo "  Service: ${SYSTEMD_PATH}"
    echo ""
    
    confirm_action "Proceed with installation?"
    
    # Step 1: Copy binary
    echo -e "${GREEN}[1/5] Copying binary to ${INSTALL_PATH}...${NC}"
    cp "${SOURCE_BINARY}" "${INSTALL_PATH}"
    chmod +x "${INSTALL_PATH}"
    
    # Step 2: Create config directory
    echo -e "${GREEN}[2/5] Creating config directory ${CONFIG_DIR}...${NC}"
    mkdir -p "${CONFIG_DIR}"
    
    # Step 3: Copy config if it doesn't exist
    if [[ ! -f "${CONFIG_FILE}" ]]; then
        echo -e "${GREEN}[3/5] Copying default configuration...${NC}"
        if [[ -f "config.example.toml" ]]; then
            cp "config.example.toml" "${CONFIG_FILE}"
            echo -e "  Created: ${CONFIG_FILE}"
        else
            echo -e "${YELLOW}  Warning: config.example.toml not found, skipping config copy${NC}"
        fi
    else
        echo -e "${GREEN}[3/5] Configuration already exists, skipping...${NC}"
        echo -e "  Existing: ${CONFIG_FILE}"
    fi
    
    # Step 4: Install systemd service
    echo -e "${GREEN}[4/5] Installing systemd service...${NC}"
    if [[ -f "${SERVICE_FILE}" ]]; then
        cp "${SERVICE_FILE}" "${SYSTEMD_PATH}"
        systemctl daemon-reload
        echo -e "  Installed: ${SYSTEMD_PATH}"
    else
        echo -e "${YELLOW}  Warning: ${SERVICE_FILE} not found, skipping service install${NC}"
    fi
    
    # Step 5: Enable service
    echo -e "${GREEN}[5/5] Enabling service...${NC}"
    systemctl enable mytunnel 2>/dev/null
    
    echo ""
    echo -e "${GREEN}Installation complete!${NC}"
    echo ""
    echo -e "${CYAN}Next steps:${NC}"
    echo "  1. Edit configuration: ${CONFIG_FILE}"
    echo "  2. Start service: ${SCRIPT_NAME} start"
    echo "  3. Check status: ${SCRIPT_NAME} status"
}

cmd_start() {
    echo -e "${BLUE}[START] Starting MyTunnel service...${NC}"
    
    check_root "start"
    
    systemctl start mytunnel
    
    if [[ $? -eq 0 ]]; then
        sleep 1
        if systemctl is-active --quiet mytunnel; then
            echo -e "${GREEN}Service started successfully!${NC}"
        else
            echo -e "${YELLOW}Service may have failed to start. Check status:${NC}"
            systemctl status mytunnel --no-pager -l
        fi
    else
        echo -e "${RED}Failed to start service${NC}"
        exit 1
    fi
}

cmd_stop() {
    echo -e "${BLUE}[STOP] Stopping MyTunnel service...${NC}"
    
    check_root "stop"
    
    if ! systemctl is-active --quiet mytunnel; then
        echo -e "${YELLOW}Service is not running${NC}"
        return 0
    fi
    
    confirm_action "Stop the mytunnel service?"
    
    systemctl stop mytunnel
    
    if [[ $? -eq 0 ]]; then
        echo -e "${GREEN}Service stopped${NC}"
    else
        echo -e "${RED}Failed to stop service${NC}"
        exit 1
    fi
}

cmd_restart() {
    echo -e "${BLUE}[RESTART] Restarting MyTunnel service...${NC}"
    
    check_root "restart"
    
    systemctl restart mytunnel
    
    if [[ $? -eq 0 ]]; then
        sleep 1
        if systemctl is-active --quiet mytunnel; then
            echo -e "${GREEN}Service restarted successfully!${NC}"
        else
            echo -e "${YELLOW}Service may have failed to restart. Check status:${NC}"
            systemctl status mytunnel --no-pager -l
        fi
    else
        echo -e "${RED}Failed to restart service${NC}"
        exit 1
    fi
}

cmd_status() {
    echo -e "${BLUE}[STATUS] MyTunnel Service Information${NC}"
    echo ""
    
    # Binary info
    echo -e "${CYAN}Binary:${NC}"
    if [[ -f "${INSTALL_PATH}" ]]; then
        echo -e "  Installed: ${GREEN}Yes${NC}"
        echo -e "  Path: ${INSTALL_PATH}"
        echo -e "  Size: $(ls -lh "${INSTALL_PATH}" 2>/dev/null | awk '{print $5}')"
    else
        echo -e "  Installed: ${RED}No${NC}"
    fi
    echo ""
    
    # Config info
    echo -e "${CYAN}Configuration:${NC}"
    if [[ -f "${CONFIG_FILE}" ]]; then
        echo -e "  Exists: ${GREEN}Yes${NC}"
        echo -e "  Path: ${CONFIG_FILE}"
    else
        echo -e "  Exists: ${RED}No${NC}"
    fi
    echo ""
    
    # Service info
    echo -e "${CYAN}Service:${NC}"
    if [[ -f "${SYSTEMD_PATH}" ]]; then
        echo -e "  Installed: ${GREEN}Yes${NC}"
        
        local status=$(systemctl is-active mytunnel 2>/dev/null)
        if [[ "$status" == "active" ]]; then
            echo -e "  Status: ${GREEN}Running${NC}"
        elif [[ "$status" == "inactive" ]]; then
            echo -e "  Status: ${YELLOW}Stopped${NC}"
        else
            echo -e "  Status: ${RED}${status}${NC}"
        fi
        
        local enabled=$(systemctl is-enabled mytunnel 2>/dev/null)
        if [[ "$enabled" == "enabled" ]]; then
            echo -e "  Enabled: ${GREEN}Yes${NC}"
        else
            echo -e "  Enabled: ${YELLOW}No${NC}"
        fi
    else
        echo -e "  Installed: ${RED}No${NC}"
    fi
    echo ""
    
    # Show systemd status if service exists
    if [[ -f "${SYSTEMD_PATH}" ]]; then
        echo -e "${CYAN}Service Details:${NC}"
        systemctl status mytunnel --no-pager -l 2>/dev/null || echo "  (service not found)"
    fi
}

#-------------------------------------------------------------------------------
# Main Entry Point
#-------------------------------------------------------------------------------

print_header

if [[ $# -eq 0 ]]; then
    print_usage
    exit 0
fi

COMMAND="$1"

case "$COMMAND" in
    build)
        cmd_build
        ;;
    install)
        cmd_install
        ;;
    start)
        cmd_start
        ;;
    stop)
        cmd_stop
        ;;
    restart)
        cmd_restart
        ;;
    status)
        cmd_status
        ;;
    -h|--help|help)
        print_usage
        ;;
    *)
        echo -e "${RED}Unknown command: ${COMMAND}${NC}"
        echo ""
        print_usage
        exit 1
        ;;
esac

