#!/bin/bash
#===============================================================================
# MyTunnel Management Script
# Version: 1.2.0
# Description: Build, install, and manage the MyTunnel service locally
#===============================================================================

VERSION="1.2.0"
SCRIPT_NAME=$(basename "$0")

# Paths
BINARY_NAME="mytunnel-server"
SOURCE_BINARY="target/release/${BINARY_NAME}"
INSTALL_PATH="/usr/local/bin/${BINARY_NAME}"
CONFIG_DIR="/etc/mytunnel"
CONFIG_FILE="${CONFIG_DIR}/config.toml"
SERVICE_FILE="mytunnel.service"
SYSTEMD_PATH="/etc/systemd/system/${SERVICE_FILE}"
API_ADDR="127.0.0.1:9091"

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
    echo "  logs      Attach to live service logs (Ctrl+C to exit)"
    echo "  users     List connected users"
    echo "  monitor   Live dashboard of connected users (Ctrl+C to exit)"
    echo ""
    echo -e "${CYAN}Examples:${NC}"
    echo "  ${SCRIPT_NAME} build"
    echo "  ${SCRIPT_NAME} install"
    echo "  ${SCRIPT_NAME} status"
    echo "  ${SCRIPT_NAME} logs"
    echo "  ${SCRIPT_NAME} logs 50      # Show last 50 lines + live"
    echo "  ${SCRIPT_NAME} users"
    echo "  ${SCRIPT_NAME} monitor"
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

format_bytes() {
    local bytes=$1
    if [[ $bytes -ge 1073741824 ]]; then
        echo "$(awk "BEGIN {printf \"%.2f\", $bytes/1073741824}")GB"
    elif [[ $bytes -ge 1048576 ]]; then
        echo "$(awk "BEGIN {printf \"%.2f\", $bytes/1048576}")MB"
    elif [[ $bytes -ge 1024 ]]; then
        echo "$(awk "BEGIN {printf \"%.2f\", $bytes/1024}")KB"
    else
        echo "${bytes}B"
    fi
}

format_duration() {
    local secs=$1
    local int_secs=${secs%.*}
    if [[ $int_secs -ge 3600 ]]; then
        printf "%dh%dm" $((int_secs/3600)) $(((int_secs%3600)/60))
    elif [[ $int_secs -ge 60 ]]; then
        printf "%dm%ds" $((int_secs/60)) $((int_secs%60))
    else
        printf "%ds" $int_secs
    fi
}

check_api_available() {
    if ! command -v curl &> /dev/null; then
        echo -e "${RED}ERROR: curl is required but not installed${NC}"
        exit 1
    fi
    
    if ! curl -s --connect-timeout 2 "http://${API_ADDR}/" &>/dev/null; then
        echo -e "${RED}ERROR: Cannot connect to API at ${API_ADDR}${NC}"
        echo -e "${YELLOW}Make sure the mytunnel service is running with metrics enabled${NC}"
        exit 1
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

cmd_logs() {
    local lines="${1:-100}"
    
    echo -e "${BLUE}[LOGS] Attaching to MyTunnel service logs...${NC}"
    echo -e "${CYAN}MyTunnel Manager v${VERSION}${NC}"
    echo -e "Showing last ${lines} lines, then following live logs"
    echo -e "${YELLOW}Press Ctrl+C to exit${NC}"
    echo ""
    echo -e "${BLUE}----------------------------------------${NC}"
    
    # Check if service exists
    if ! systemctl list-unit-files mytunnel.service &>/dev/null; then
        echo -e "${RED}ERROR: mytunnel service not found${NC}"
        echo -e "${YELLOW}Run '${SCRIPT_NAME} install' first${NC}"
        exit 1
    fi
    
    # Attach to live logs with journalctl
    # -u: filter by unit
    # -f: follow (like tail -f)
    # -n: number of previous lines to show
    # --no-pager: don't use pager, stream directly
    journalctl -u mytunnel -f -n "${lines}" --no-pager
}

cmd_users() {
    echo -e "${BLUE}[USERS] Connected Users${NC}"
    echo -e "${CYAN}MyTunnel Manager v${VERSION}${NC}"
    echo ""
    
    check_api_available
    
    # Fetch connections from API
    local response
    response=$(curl -s "http://${API_ADDR}/connections" 2>/dev/null)
    
    if [[ -z "$response" ]]; then
        echo -e "${RED}ERROR: Failed to fetch connections${NC}"
        exit 1
    fi
    
    local count
    count=$(echo "$response" | grep -o '"count":[0-9]*' | cut -d: -f2)
    
    if [[ "$count" == "0" || -z "$count" ]]; then
        echo -e "${YELLOW}No users currently connected${NC}"
        return 0
    fi
    
    echo -e "${GREEN}Active Connections: ${count}${NC}"
    echo ""
    
    # Print table header
    printf "${CYAN}%-18s %-22s %-10s %-12s %-12s %-8s${NC}\n" \
        "ID" "CLIENT IP" "DURATION" "RX" "TX" "STREAMS"
    echo "--------------------------------------------------------------------------------"
    
    # Parse and display connections using simple parsing
    echo "$response" | grep -oP '\{[^}]+\}' | while read -r conn; do
        local id client_addr duration_secs bytes_rx bytes_tx streams
        
        id=$(echo "$conn" | grep -oP '"id"\s*:\s*"\K[^"]+')
        client_addr=$(echo "$conn" | grep -oP '"client_addr"\s*:\s*"\K[^"]+')
        duration_secs=$(echo "$conn" | grep -oP '"duration_secs"\s*:\s*\K[0-9.]+')
        bytes_rx=$(echo "$conn" | grep -oP '"bytes_rx"\s*:\s*\K[0-9]+')
        bytes_tx=$(echo "$conn" | grep -oP '"bytes_tx"\s*:\s*\K[0-9]+')
        streams=$(echo "$conn" | grep -oP '"active_streams"\s*:\s*\K[0-9]+')
        
        # Format values
        local duration_fmt rx_fmt tx_fmt
        duration_fmt=$(format_duration "${duration_secs:-0}")
        rx_fmt=$(format_bytes "${bytes_rx:-0}")
        tx_fmt=$(format_bytes "${bytes_tx:-0}")
        
        # Truncate ID for display
        local id_short="${id:0:16}"
        
        printf "%-18s %-22s %-10s %-12s %-12s %-8s\n" \
            "${id_short}" "${client_addr}" "${duration_fmt}" "${rx_fmt}" "${tx_fmt}" "${streams:-0}"
    done
    
    echo ""
}

cmd_monitor() {
    echo -e "${BLUE}[MONITOR] Live User Dashboard${NC}"
    echo -e "${CYAN}MyTunnel Manager v${VERSION}${NC}"
    echo -e "${YELLOW}Press Ctrl+C to exit${NC}"
    echo ""
    
    check_api_available
    
    # Monitor loop
    while true; do
        # Clear screen
        clear
        
        echo -e "${BLUE}========================================${NC}"
        echo -e "${BLUE}  MyTunnel Live Monitor v${VERSION}${NC}"
        echo -e "${BLUE}========================================${NC}"
        echo -e "${YELLOW}Press Ctrl+C to exit | Refresh: 2s${NC}"
        echo ""
        
        # Fetch stats
        local stats
        stats=$(curl -s "http://${API_ADDR}/stats" 2>/dev/null)
        
        if [[ -n "$stats" ]]; then
            local total active failed bytes_rx bytes_tx
            total=$(echo "$stats" | grep -oP '"connections_total"\s*:\s*\K[0-9]+')
            active=$(echo "$stats" | grep -oP '"connections_active"\s*:\s*\K[0-9]+')
            failed=$(echo "$stats" | grep -oP '"connections_failed"\s*:\s*\K[0-9]+')
            bytes_rx=$(echo "$stats" | grep -oP '"bytes_received"\s*:\s*\K[0-9]+')
            bytes_tx=$(echo "$stats" | grep -oP '"bytes_sent"\s*:\s*\K[0-9]+')
            
            echo -e "${CYAN}Server Statistics:${NC}"
            printf "  Total Connections:  ${GREEN}%s${NC}\n" "${total:-0}"
            printf "  Active Connections: ${GREEN}%s${NC}\n" "${active:-0}"
            printf "  Failed Connections: ${YELLOW}%s${NC}\n" "${failed:-0}"
            printf "  Total RX:           ${GREEN}%s${NC}\n" "$(format_bytes "${bytes_rx:-0}")"
            printf "  Total TX:           ${GREEN}%s${NC}\n" "$(format_bytes "${bytes_tx:-0}")"
            echo ""
        fi
        
        # Fetch and display connections
        local response
        response=$(curl -s "http://${API_ADDR}/connections" 2>/dev/null)
        
        local count
        count=$(echo "$response" | grep -o '"count":[0-9]*' | cut -d: -f2)
        
        echo -e "${CYAN}Connected Users (${count:-0}):${NC}"
        echo ""
        
        if [[ "$count" == "0" || -z "$count" ]]; then
            echo -e "  ${YELLOW}No users currently connected${NC}"
        else
            # Print table header
            printf "  ${CYAN}%-18s %-22s %-10s %-12s %-12s %-8s${NC}\n" \
                "ID" "CLIENT IP" "DURATION" "RX" "TX" "STREAMS"
            echo "  ------------------------------------------------------------------------------"
            
            # Parse and display connections
            echo "$response" | grep -oP '\{[^}]+\}' | while read -r conn; do
                local id client_addr duration_secs bytes_rx bytes_tx streams
                
                id=$(echo "$conn" | grep -oP '"id"\s*:\s*"\K[^"]+')
                client_addr=$(echo "$conn" | grep -oP '"client_addr"\s*:\s*"\K[^"]+')
                duration_secs=$(echo "$conn" | grep -oP '"duration_secs"\s*:\s*\K[0-9.]+')
                bytes_rx=$(echo "$conn" | grep -oP '"bytes_rx"\s*:\s*\K[0-9]+')
                bytes_tx=$(echo "$conn" | grep -oP '"bytes_tx"\s*:\s*\K[0-9]+')
                streams=$(echo "$conn" | grep -oP '"active_streams"\s*:\s*\K[0-9]+')
                
                local duration_fmt rx_fmt tx_fmt id_short
                duration_fmt=$(format_duration "${duration_secs:-0}")
                rx_fmt=$(format_bytes "${bytes_rx:-0}")
                tx_fmt=$(format_bytes "${bytes_tx:-0}")
                id_short="${id:0:16}"
                
                printf "  %-18s %-22s %-10s %-12s %-12s %-8s\n" \
                    "${id_short}" "${client_addr}" "${duration_fmt}" "${rx_fmt}" "${tx_fmt}" "${streams:-0}"
            done
        fi
        
        echo ""
        echo -e "${BLUE}----------------------------------------${NC}"
        echo -e "Last updated: $(date '+%Y-%m-%d %H:%M:%S')"
        
        sleep 2
    done
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
    logs)
        cmd_logs "$2"
        ;;
    users)
        cmd_users
        ;;
    monitor)
        cmd_monitor
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

