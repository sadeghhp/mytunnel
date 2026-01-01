#!/bin/bash
#===============================================================================
# MyTunnel Client Management Script
# Version: 1.0.0
# Description: Build, configure, test, and troubleshoot MyTunnel client
#===============================================================================

VERSION="1.0.8"
SCRIPT_NAME=$(basename "$0")
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Paths
BINARY_NAME="mytunnel-client"
SOURCE_BINARY="${SCRIPT_DIR}/target/release/${BINARY_NAME}"
CONFIG_FILE="${SCRIPT_DIR}/client-config.toml"
CONFIG_EXAMPLE="${SCRIPT_DIR}/client-config.example.toml"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

# Platform detection
OS_TYPE=""
detect_platform() {
    case "$(uname -s)" in
        Darwin*)
            OS_TYPE="macos"
            ;;
        Linux*)
            OS_TYPE="linux"
            ;;
        *)
            OS_TYPE="unknown"
            ;;
    esac
}

#-------------------------------------------------------------------------------
# Helper Functions
#-------------------------------------------------------------------------------

print_header() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}  MyTunnel Client Manager v${VERSION}${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo -e "  Platform: ${CYAN}${OS_TYPE}${NC}"
    echo ""
}

print_usage() {
    echo -e "${CYAN}Usage:${NC} ${SCRIPT_NAME} <command>"
    echo ""
    echo -e "${CYAN}Build & Setup:${NC}"
    echo "  build         Build the release binary"
    echo "  setup         Interactive configuration wizard"
    echo "  config        Show/validate current configuration"
    echo ""
    echo -e "${CYAN}Connection Testing:${NC}"
    echo "  test          Run full connectivity test suite"
    echo "  test-dns      Test DNS resolution for server"
    echo "  test-port     Test TCP port reachability"
    echo "  test-tls      Test TLS certificate validity"
    echo "  test-quic     Test QUIC handshake with server"
    echo "  test-proxy    Test SOCKS5/HTTP proxy functionality"
    echo ""
    echo -e "${CYAN}Diagnostics:${NC}"
    echo "  diagnose      Run comprehensive diagnostics"
    echo ""
    echo -e "${CYAN}Examples:${NC}"
    echo "  ${SCRIPT_NAME} build"
    echo "  ${SCRIPT_NAME} setup"
    echo "  ${SCRIPT_NAME} test"
    echo "  ${SCRIPT_NAME} diagnose"
}

print_step() {
    echo -e "${GREEN}[*]${NC} $1"
}

print_info() {
    echo -e "${CYAN}[i]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[✓]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

print_error() {
    echo -e "${RED}[✗]${NC} $1"
}

print_section() {
    echo ""
    echo -e "${MAGENTA}── $1 ──${NC}"
}

# Check if a command exists
cmd_exists() {
    command -v "$1" &> /dev/null
}

# Cross-platform timeout command
run_with_timeout() {
    local timeout_secs="$1"
    shift
    
    if [[ "$OS_TYPE" == "macos" ]]; then
        if cmd_exists gtimeout; then
            gtimeout "$timeout_secs" "$@"
        else
            # Fallback: run without timeout on macOS if gtimeout not available
            "$@"
        fi
    else
        timeout "$timeout_secs" "$@"
    fi
}

# Cross-platform netcat port check
check_port_nc() {
    local host="$1"
    local port="$2"
    local timeout_secs="${3:-5}"
    
    if [[ "$OS_TYPE" == "macos" ]]; then
        nc -z -G "$timeout_secs" "$host" "$port" 2>/dev/null
    else
        nc -z -w "$timeout_secs" "$host" "$port" 2>/dev/null
    fi
}

check_cargo() {
    if ! cmd_exists cargo; then
        print_error "Cargo is not installed or not in PATH"
        echo -e "${YELLOW}Install Rust from: https://rustup.rs${NC}"
        return 1
    fi
    return 0
}

check_binary_exists() {
    if [[ ! -f "${SOURCE_BINARY}" ]]; then
        print_error "Binary not found at ${SOURCE_BINARY}"
        echo -e "${YELLOW}Run '${SCRIPT_NAME} build' first${NC}"
        return 1
    fi
    return 0
}

check_config_exists() {
    if [[ ! -f "${CONFIG_FILE}" ]]; then
        print_error "Configuration not found at ${CONFIG_FILE}"
        echo -e "${YELLOW}Run '${SCRIPT_NAME} setup' to create configuration${NC}"
        return 1
    fi
    return 0
}

# Extract values from TOML config
get_config_value() {
    local key="$1"
    local value
    # Extract value after the = sign
    value=$(grep -E "^${key}\s*=" "${CONFIG_FILE}" 2>/dev/null | head -1)
    # Remove key and equals sign
    value="${value#*=}"
    # Trim leading whitespace using sed (more portable)
    value=$(echo "$value" | sed 's/^[[:space:]]*//' | sed 's/[[:space:]]*$//')
    # Remove surrounding double quotes if present
    value="${value#\"}"
    value="${value%\"}"
    echo "$value"
}

# Get server host and port from config
get_server_host() {
    local address=$(get_config_value "address")
    echo "${address%:*}"
}

get_server_port() {
    local address=$(get_config_value "address")
    echo "${address##*:}"
}

confirm_action() {
    local message="$1"
    local default="${2:-n}"
    
    if [[ "$default" == "y" ]]; then
        read -p "${message} [Y/n]: " -r response
        response=${response:-y}
    else
        read -p "${message} [y/N]: " -r response
        response=${response:-n}
    fi
    
    [[ "$response" =~ ^[Yy]$ ]]
}

#-------------------------------------------------------------------------------
# Build Command
#-------------------------------------------------------------------------------

cmd_build() {
    print_section "Building MyTunnel Client"
    
    check_cargo || return 1
    
    print_info "Cargo version: $(cargo --version)"
    print_step "Compiling release binary..."
    echo ""
    
    cd "${SCRIPT_DIR}" || return 1
    cargo build --release
    
    if [[ $? -eq 0 ]]; then
        echo ""
        print_success "Build successful!"
        print_info "Binary: ${SOURCE_BINARY}"
        print_info "Size: $(ls -lh "${SOURCE_BINARY}" 2>/dev/null | awk '{print $5}')"
        
        # Show version if binary can be executed
        if [[ -x "${SOURCE_BINARY}" ]]; then
            local ver=$("${SOURCE_BINARY}" --version 2>/dev/null | head -1)
            print_info "Version: ${ver}"
        fi
    else
        echo ""
        print_error "Build failed!"
        return 1
    fi
}

#-------------------------------------------------------------------------------
# Setup Command (Interactive Configuration Wizard)
#-------------------------------------------------------------------------------

cmd_setup() {
    print_section "Configuration Setup Wizard"
    
    if [[ -f "${CONFIG_FILE}" ]]; then
        print_warning "Configuration file already exists: ${CONFIG_FILE}"
        if ! confirm_action "Overwrite existing configuration?"; then
            print_info "Setup cancelled"
            return 0
        fi
    fi
    
    echo ""
    echo -e "${CYAN}This wizard will help you configure MyTunnel client.${NC}"
    echo ""
    
    # Server address
    local server_address=""
    local server_host=""
    local server_port=""
    
    while [[ -z "$server_address" ]]; do
        echo -e "${CYAN}Enter the tunnel server address.${NC}"
        echo -e "Examples: ${GREEN}tunnel.example.com:443${NC}, ${GREEN}192.168.1.100:4433${NC}"
        echo ""
        read -p "Server address (host:port): " server_address
        
        if [[ -z "$server_address" ]]; then
            print_error "Server address is required"
            continue
        fi
        
        # Parse host and port
        if [[ "$server_address" =~ : ]]; then
            server_host="${server_address%:*}"
            server_port="${server_address##*:}"
        else
            server_host="$server_address"
            server_port=""
        fi
        
        # Validate host is not empty and not just a number (port only)
        if [[ -z "$server_host" ]]; then
            print_error "Invalid address: missing hostname"
            server_address=""
            continue
        fi
        
        # Check if user entered just a port number
        if [[ "$server_host" =~ ^[0-9]+$ ]] && [[ ${#server_host} -le 5 ]]; then
            print_error "Invalid address: '${server_host}' looks like a port number, not a hostname"
            print_info "Please enter hostname or IP address (e.g., tunnel.example.com or 192.168.1.100)"
            server_address=""
            continue
        fi
        
        # Add default port if not specified
        if [[ -z "$server_port" ]]; then
            print_warning "No port specified, using default :443"
            server_address="${server_host}:443"
        fi
        
        # Validate port is numeric
        server_port="${server_address##*:}"
        if ! [[ "$server_port" =~ ^[0-9]+$ ]]; then
            print_error "Invalid port: '${server_port}' is not a number"
            server_address=""
            continue
        fi
        
        print_success "Server: ${server_address}"
    done
    
    # TLS settings
    echo ""
    echo -e "${CYAN}TLS Settings:${NC}"
    echo "  1) Production (verify certificates)"
    echo "  2) Development (skip certificate verification - INSECURE)"
    read -p "Select TLS mode [1]: " tls_mode
    tls_mode=${tls_mode:-1}
    
    local insecure="false"
    if [[ "$tls_mode" == "2" ]]; then
        insecure="true"
        print_warning "Certificate verification will be disabled (insecure mode)"
    fi
    
    # SOCKS5 proxy
    echo ""
    read -p "SOCKS5 proxy bind address [127.0.0.1:1080]: " socks5_bind
    socks5_bind=${socks5_bind:-127.0.0.1:1080}
    
    # HTTP proxy
    read -p "HTTP proxy bind address [127.0.0.1:8080]: " http_bind
    http_bind=${http_bind:-127.0.0.1:8080}
    
    # Enable/disable proxies
    echo ""
    local socks5_enabled="true"
    local http_enabled="true"
    
    if ! confirm_action "Enable SOCKS5 proxy?" "y"; then
        socks5_enabled="false"
    fi
    
    if ! confirm_action "Enable HTTP proxy?" "y"; then
        http_enabled="false"
    fi
    
    # Log level
    echo ""
    echo -e "${CYAN}Log Level:${NC}"
    echo "  1) info (recommended)"
    echo "  2) debug"
    echo "  3) trace (verbose)"
    echo "  4) warn"
    echo "  5) error"
    read -p "Select log level [1]: " log_level_choice
    
    local log_level="info"
    case "$log_level_choice" in
        2) log_level="debug" ;;
        3) log_level="trace" ;;
        4) log_level="warn" ;;
        5) log_level="error" ;;
        *) log_level="info" ;;
    esac
    
    # Generate configuration
    print_section "Generating Configuration"
    
    cat > "${CONFIG_FILE}" << EOF
# MyTunnel Client Configuration
# Generated by client-manage.sh v${VERSION}

[server]
# Server address (host:port)
address = "${server_address}"
# Skip TLS certificate verification (INSECURE, dev only!)
insecure = ${insecure}

[proxy]
# SOCKS5 proxy bind address
socks5_bind = "${socks5_bind}"
# HTTP proxy bind address
http_bind = "${http_bind}"
# Enable SOCKS5 proxy
socks5_enabled = ${socks5_enabled}
# Enable HTTP proxy
http_enabled = ${http_enabled}

[quic]
# Connection idle timeout in seconds
idle_timeout_secs = 30
# Enable 0-RTT for faster reconnection
enable_0rtt = true
# Maximum concurrent streams
max_streams = 100

[logging]
# Log level: trace, debug, info, warn, error
level = "${log_level}"
# Output format: "json" or "pretty"
format = "pretty"
EOF
    
    print_success "Configuration saved to: ${CONFIG_FILE}"
    echo ""
    
    # Offer to test connection
    if confirm_action "Test connection to server now?" "y"; then
        cmd_test_quic
    fi
}

#-------------------------------------------------------------------------------
# Config Command
#-------------------------------------------------------------------------------

cmd_config() {
    print_section "Configuration"
    
    if ! check_config_exists; then
        return 1
    fi
    
    print_info "Config file: ${CONFIG_FILE}"
    echo ""
    
    # Display current configuration
    echo -e "${CYAN}Current Settings:${NC}"
    echo "─────────────────────────────────────"
    cat "${CONFIG_FILE}" | grep -v "^#" | grep -v "^$"
    echo "─────────────────────────────────────"
    echo ""
    
    # Validate with client binary if available
    if check_binary_exists 2>/dev/null; then
        print_step "Validating configuration..."
        
        # Try to load config via client (will fail if invalid)
        if "${SOURCE_BINARY}" test-connection -c "${CONFIG_FILE}" --help &>/dev/null; then
            print_success "Configuration syntax is valid"
        fi
    fi
}

#-------------------------------------------------------------------------------
# DNS Test Command
#-------------------------------------------------------------------------------

cmd_test_dns() {
    print_section "DNS Resolution Test"
    
    if ! check_config_exists; then
        return 1
    fi
    
    local host=$(get_server_host)
    
    if [[ -z "$host" ]]; then
        print_error "Could not extract server host from config"
        return 1
    fi
    
    print_info "Host: ${host}"
    
    # Check if host is already an IP address (IPv4)
    if [[ "$host" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        print_success "Host is an IP address - DNS resolution not needed"
        print_info "IP: ${host}"
        return 0
    fi
    
    # Check if host is IPv6
    if [[ "$host" =~ : ]] && [[ "$host" =~ ^[0-9a-fA-F:]+$ ]]; then
        print_success "Host is an IPv6 address - DNS resolution not needed"
        print_info "IP: ${host}"
        return 0
    fi
    
    echo ""
    print_step "Resolving hostname..."
    
    local resolved=""
    local dns_tool=""
    
    # Try different DNS tools
    if cmd_exists dig; then
        dns_tool="dig"
        print_step "Using dig..."
        resolved=$(dig +short "$host" 2>/dev/null | head -1)
        dig +short "$host" 2>/dev/null
    elif cmd_exists host; then
        dns_tool="host"
        print_step "Using host..."
        host "$host" 2>/dev/null
        resolved=$(host "$host" 2>/dev/null | grep "has address" | head -1 | awk '{print $NF}')
    elif cmd_exists nslookup; then
        dns_tool="nslookup"
        print_step "Using nslookup..."
        nslookup "$host" 2>/dev/null
        resolved=$(nslookup "$host" 2>/dev/null | grep "Address:" | tail -1 | awk '{print $2}')
    elif cmd_exists getent && [[ "$OS_TYPE" == "linux" ]]; then
        dns_tool="getent"
        print_step "Using getent..."
        getent hosts "$host" 2>/dev/null
        resolved=$(getent hosts "$host" 2>/dev/null | awk '{print $1}')
    else
        print_error "No DNS lookup tool found (dig, host, nslookup, getent)"
        return 1
    fi
    
    echo ""
    if [[ -n "$resolved" ]]; then
        print_success "DNS resolution successful"
        print_info "Resolved to: ${resolved}"
        return 0
    else
        print_error "DNS resolution failed"
        print_info "Check if the hostname is correct"
        return 1
    fi
}

#-------------------------------------------------------------------------------
# Port Test Command
#-------------------------------------------------------------------------------

cmd_test_port() {
    print_section "TCP Port Reachability Test (Informational)"
    
    if ! check_config_exists; then
        return 1
    fi
    
    local host=$(get_server_host)
    local port=$(get_server_port)
    
    if [[ -z "$host" || -z "$port" ]]; then
        print_error "Could not extract server host:port from config"
        return 1
    fi
    
    print_info "Testing TCP connect to: ${host}:${port}"
    print_info "Note: MyTunnel uses QUIC over UDP; TCP port checks may fail even when QUIC works."
    
    local start_time=$(date +%s%N 2>/dev/null || date +%s)
    
    if cmd_exists nc; then
        print_step "Using netcat..."
        if check_port_nc "$host" "$port" 5; then
            local end_time=$(date +%s%N 2>/dev/null || date +%s)
            print_success "Port ${port} is reachable"
            
            # Calculate latency if nanoseconds available
            if [[ "$start_time" =~ ^[0-9]+$ && ${#start_time} -gt 10 ]]; then
                local latency=$(( (end_time - start_time) / 1000000 ))
                print_info "TCP connect time: ~${latency}ms"
            fi
            return 0
        else
            print_warning "TCP port ${port} is not reachable"
            print_info "This is expected if the server is QUIC-only (UDP) and does not accept TCP on this port."
            print_info "If you expect TCP/HTTPS on this port, possible causes:"
            echo "  - Server is not running"
            echo "  - Firewall blocking TCP"
            echo "  - Wrong port number"
            return 1
        fi
    elif cmd_exists curl; then
        print_step "Using curl (fallback)..."
        if run_with_timeout 5 curl -s --connect-timeout 5 "telnet://${host}:${port}" &>/dev/null; then
            print_success "Port ${port} appears reachable"
            return 0
        else
            print_warning "TCP port ${port} is not reachable"
            print_info "This check is informational (TLS/QUIC typically do not use TCP sockets)."
            return 1
        fi
    else
        print_error "No suitable tool found (nc or curl required)"
        return 1
    fi
}

#-------------------------------------------------------------------------------
# TLS Test Command
#-------------------------------------------------------------------------------

cmd_test_tls() {
    print_section "TLS Certificate Test (TLS-over-TCP, Informational)"
    
    if ! check_config_exists; then
        return 1
    fi
    
    local host=$(get_server_host)
    local port=$(get_server_port)
    local insecure=$(get_config_value "insecure")
    
    if [[ -z "$host" || -z "$port" ]]; then
        print_error "Could not extract server host:port from config"
        return 1
    fi
    
    print_info "Note: This uses TLS-over-TCP (openssl s_client). QUIC uses TLS 1.3 inside UDP."
    
    if [[ "$insecure" == "true" ]]; then
        print_warning "Insecure mode enabled - certificate verification is disabled"
        print_info "TLS test will still check if server presents a certificate"
    fi
    
    print_info "Testing: ${host}:${port}"
    
    if ! cmd_exists openssl; then
        print_error "OpenSSL not found"
        print_info "Install openssl to run TLS tests"
        return 1
    fi
    
    print_step "Connecting with OpenSSL..."
    echo ""
    
    # Get certificate info
    local cert_info=$(echo | run_with_timeout 10 openssl s_client -connect "${host}:${port}" -servername "${host}" 2>/dev/null)
    
    if [[ -z "$cert_info" ]]; then
        print_warning "Could not establish TLS-over-TCP connection"
        print_info "This is expected for QUIC-only servers. QUIC handshake test is the authoritative check."
        return 1
    fi
    
    # Check if certificate was received
    if echo "$cert_info" | grep -q "BEGIN CERTIFICATE"; then
        print_success "Server presented a certificate"
        
        # Extract and display certificate details
        echo ""
        echo -e "${CYAN}Certificate Details:${NC}"
        echo "$cert_info" | openssl x509 -noout -subject -issuer -dates 2>/dev/null | while read line; do
            echo "  $line"
        done
        
        # Check expiry
        local not_after=$(echo "$cert_info" | openssl x509 -noout -enddate 2>/dev/null | cut -d= -f2)
        if [[ -n "$not_after" ]]; then
            local expiry_epoch=$(date -d "$not_after" +%s 2>/dev/null || date -j -f "%b %d %T %Y %Z" "$not_after" +%s 2>/dev/null)
            local now_epoch=$(date +%s)
            
            if [[ -n "$expiry_epoch" && -n "$now_epoch" ]]; then
                local days_left=$(( (expiry_epoch - now_epoch) / 86400 ))
                
                if [[ $days_left -lt 0 ]]; then
                    print_error "Certificate has EXPIRED!"
                elif [[ $days_left -lt 30 ]]; then
                    print_warning "Certificate expires in ${days_left} days"
                else
                    print_success "Certificate valid for ${days_left} days"
                fi
            fi
        fi
        
        # Verify chain
        echo ""
        local verify_result=$(echo | run_with_timeout 10 openssl s_client -connect "${host}:${port}" -servername "${host}" -verify_return_error 2>&1 | grep "Verify return code")
        
        if echo "$verify_result" | grep -q "0 (ok)"; then
            print_success "Certificate chain verified successfully"
        else
            local verify_code=$(echo "$verify_result" | grep -oE "[0-9]+ \([^)]+\)")
            print_warning "Certificate verification: ${verify_code}"
            if [[ "$insecure" == "true" ]]; then
                print_info "This is expected since insecure mode is enabled"
            fi
        fi
        
        return 0
    else
        print_error "No certificate received from server"
        return 1
    fi
}

#-------------------------------------------------------------------------------
# QUIC Test Command
#-------------------------------------------------------------------------------

cmd_test_quic() {
    print_section "QUIC Connection Test"
    
    if ! check_config_exists; then
        return 1
    fi
    
    if ! check_binary_exists; then
        return 1
    fi
    
    local host=$(get_server_host)
    local port=$(get_server_port)
    
    print_info "Server: ${host}:${port}"
    print_step "Testing QUIC handshake..."
    echo ""
    
    # Run the client's built-in test-connection command
    "${SOURCE_BINARY}" test-connection -c "${CONFIG_FILE}"
    local result=$?
    
    echo ""
    if [[ $result -eq 0 ]]; then
        print_success "QUIC connection test passed!"
        return 0
    else
        print_error "QUIC connection test failed"
        print_info "Possible causes:"
        echo "  - Server not running or unreachable"
        echo "  - UDP traffic blocked by firewall"
        echo "  - Certificate issues (try insecure mode for testing)"
        echo "  - QUIC protocol blocked by network"
        return 1
    fi
}

#-------------------------------------------------------------------------------
# Proxy Test Command
#-------------------------------------------------------------------------------

cmd_test_proxy() {
    print_section "Proxy Functionality Test"
    
    if ! check_config_exists; then
        return 1
    fi
    
    if ! check_binary_exists; then
        return 1
    fi
    
    if ! cmd_exists curl; then
        print_error "curl is required for proxy testing"
        return 1
    fi
    
    local socks5_bind=$(get_config_value "socks5_bind")
    local http_bind=$(get_config_value "http_bind")
    local socks5_enabled=$(get_config_value "socks5_enabled")
    local http_enabled=$(get_config_value "http_enabled")
    
    # Default values if not found
    socks5_bind=${socks5_bind:-127.0.0.1:1080}
    http_bind=${http_bind:-127.0.0.1:8080}
    socks5_enabled=${socks5_enabled:-true}
    http_enabled=${http_enabled:-true}
    
    print_info "SOCKS5: ${socks5_bind} (enabled: ${socks5_enabled})"
    print_info "HTTP:   ${http_bind} (enabled: ${http_enabled})"
    echo ""
    
    # Check if client is already running by testing proxy ports
    local socks5_host="${socks5_bind%:*}"
    local socks5_port="${socks5_bind##*:}"
    local http_host="${http_bind%:*}"
    local http_port="${http_bind##*:}"
    
    local client_running=false
    if check_port_nc "$socks5_host" "$socks5_port" 1 2>/dev/null || check_port_nc "$http_host" "$http_port" 1 2>/dev/null; then
        client_running=true
        print_info "Detected running proxy on configured ports"
    fi
    
    local client_pid=""
    
    if ! $client_running; then
        print_step "Starting client in background..."

        # Check if proxy ports are already in use (common startup failure)
        if cmd_exists lsof; then
            if lsof -i ":${socks5_port}" &>/dev/null; then
                print_warning "Local port ${socks5_port} is already in use (SOCKS5 bind may fail)"
            fi
            if lsof -i ":${http_port}" &>/dev/null; then
                print_warning "Local port ${http_port} is already in use (HTTP bind may fail)"
            fi
        fi

        # Start client in background and capture logs
        local client_log
        client_log=$(mktemp -t mytunnel-client.XXXXXX.log 2>/dev/null || echo "${SCRIPT_DIR}/.mytunnel-client-test.log")
        "${SOURCE_BINARY}" run -c "${CONFIG_FILE}" >"${client_log}" 2>&1 &
        client_pid=$!

        # Wait for client to start: poll for process + proxy ports for up to ~6 seconds
        local started=false
        for _ in 1 2 3 4 5 6; do
            if ! kill -0 "$client_pid" 2>/dev/null; then
                break
            fi
            if [[ "$socks5_enabled" == "true" ]] && check_port_nc "$socks5_host" "$socks5_port" 1 2>/dev/null; then
                started=true
                break
            fi
            if [[ "$http_enabled" == "true" ]] && check_port_nc "$http_host" "$http_port" 1 2>/dev/null; then
                started=true
                break
            fi
            sleep 1
        done

        if ! $started; then
            print_error "Client failed to start"
            print_info "Last startup log lines:"
            tail -50 "${client_log}" 2>/dev/null || true
            print_info "Hint: Run '${SCRIPT_NAME} test-quic' to confirm server connectivity, and check local ports ${socks5_port}/${http_port} are free."
            return 1
        fi

        print_success "Client started (PID: ${client_pid})"
    fi
    
    # Note: The HTTP proxy in MyTunnel is typically an HTTP CONNECT proxy (tunneling),
    # so we use an HTTPS URL to force CONNECT behavior in curl.
    #
    # We also use -k/--insecure for this connectivity probe to avoid failures on systems
    # where curl can't locate CA bundles (this is a connectivity test, not a PKI audit).
    local test_url="https://example.com/"
    local all_passed=true
    
    # Test SOCKS5
    if [[ "$socks5_enabled" == "true" ]]; then
        echo ""
        print_step "Testing SOCKS5 proxy..."
        
        local socks5_start=$(date +%s%N 2>/dev/null || date +%s)
        local socks5_err
        socks5_err=$(mktemp -t mytunnel-socks5.XXXXXX.err 2>/dev/null || echo "${SCRIPT_DIR}/.mytunnel-socks5.err")
        local socks5_code
        socks5_code=$(curl -k -sS --socks5 "${socks5_bind}" --connect-timeout 10 --max-time 20 -o /dev/null -w "%{http_code}" "${test_url}" 2>"${socks5_err}")
        local socks5_exit=$?
        local socks5_end=$(date +%s%N 2>/dev/null || date +%s)
        
        if [[ $socks5_exit -eq 0 ]] && [[ "$socks5_code" =~ ^[23][0-9]{2}$ ]]; then
            if [[ "$socks5_start" =~ ^[0-9]+$ && ${#socks5_start} -gt 10 ]]; then
                local socks5_latency=$(( (socks5_end - socks5_start) / 1000000 ))
                print_success "SOCKS5 proxy working (HTTP ${socks5_code}, ${socks5_latency}ms)"
            else
                print_success "SOCKS5 proxy working (HTTP ${socks5_code})"
            fi
        else
            print_error "SOCKS5 proxy test failed"
            print_info "curl exit code: ${socks5_exit}"
            print_info "curl http_code: ${socks5_code}"
            print_info "curl stderr:"
            tail -20 "${socks5_err}" 2>/dev/null || true
            all_passed=false
        fi
    fi
    
    # Test HTTP proxy
    if [[ "$http_enabled" == "true" ]]; then
        echo ""
        print_step "Testing HTTP proxy..."
        
        local http_start=$(date +%s%N 2>/dev/null || date +%s)
        # For HTTPS targets, curl will use CONNECT through the HTTP proxy.
        local http_err
        http_err=$(mktemp -t mytunnel-http.XXXXXX.err 2>/dev/null || echo "${SCRIPT_DIR}/.mytunnel-http.err")
        local http_code
        http_code=$(curl -k -sS -x "http://${http_bind}" --connect-timeout 10 --max-time 20 -o /dev/null -w "%{http_code}" "${test_url}" 2>"${http_err}")
        local http_exit=$?
        local http_end=$(date +%s%N 2>/dev/null || date +%s)
        
        if [[ $http_exit -eq 0 ]] && [[ "$http_code" =~ ^[23][0-9]{2}$ ]]; then
            if [[ "$http_start" =~ ^[0-9]+$ && ${#http_start} -gt 10 ]]; then
                local http_latency=$(( (http_end - http_start) / 1000000 ))
                print_success "HTTP proxy working (HTTP ${http_code}, ${http_latency}ms)"
            else
                print_success "HTTP proxy working (HTTP ${http_code})"
            fi
        else
            print_error "HTTP proxy test failed"
            print_info "curl exit code: ${http_exit}"
            print_info "curl http_code: ${http_code}"
            print_info "curl stderr:"
            tail -20 "${http_err}" 2>/dev/null || true
            all_passed=false
        fi
    fi
    
    # Cleanup: stop client if we started it
    if [[ -n "$client_pid" ]]; then
        echo ""
        print_step "Stopping test client..."
        kill "$client_pid" 2>/dev/null
        wait "$client_pid" 2>/dev/null
        print_info "Client stopped"
    fi
    
    echo ""
    if $all_passed; then
        print_success "All proxy tests passed!"
        return 0
    else
        print_error "Some proxy tests failed"
        return 1
    fi
}

#-------------------------------------------------------------------------------
# Full Test Command
#-------------------------------------------------------------------------------

cmd_test() {
    print_section "Full Connectivity Test Suite"
    
    # MyTunnel uses QUIC over UDP. The authoritative connectivity check is QUIC handshake.
    # DNS is required unless you're using an IP address. TCP/TLS-over-TCP checks are informational.
    local required_run=0
    local required_passed=0
    local optional_run=0
    local optional_passed=0
    
    echo ""
    
    # DNS Test
    echo -e "${BLUE}[1/4] DNS Resolution (required)${NC}"
    ((required_run++))
    if cmd_test_dns; then
        ((required_passed++))
    fi
    
    # Port Test
    echo ""
    echo -e "${BLUE}[2/4] TCP Port Reachability (optional)${NC}"
    ((optional_run++))
    if cmd_test_port; then
        ((optional_passed++))
    fi
    
    # TLS Test
    echo ""
    echo -e "${BLUE}[3/4] TLS-over-TCP Certificate (optional)${NC}"
    ((optional_run++))
    if cmd_test_tls; then
        ((optional_passed++))
    fi
    
    # QUIC Test
    echo ""
    echo -e "${BLUE}[4/4] QUIC Handshake (required)${NC}"
    ((required_run++))
    local quic_ok=false
    if cmd_test_quic; then
        ((required_passed++))
        quic_ok=true
    fi
    
    # Summary
    print_section "Test Summary"
    echo ""
    echo -e "Required tests passed: ${required_passed}/${required_run}"
    echo -e "Optional tests passed:  ${optional_passed}/${optional_run}"
    
    if $quic_ok; then
        echo ""
        print_success "QUIC connectivity is working (this is the authoritative MyTunnel connectivity check)."
        if [[ $optional_passed -lt $optional_run ]]; then
            print_warning "Some optional TCP/TLS checks failed. This is normal for QUIC-only servers."
        fi
        print_info "Next: ${SCRIPT_NAME} test-proxy"
        return 0
    fi
    
    echo ""
    print_error "QUIC connectivity test failed"
    print_info "Troubleshooting tips:"
    echo "  - Ensure UDP is allowed to the server/port (QUIC uses UDP)"
    echo "  - Check server is running and listening on the configured port"
    echo "  - If using self-signed certs, set server.insecure = true (dev only)"
    return 1
}

#-------------------------------------------------------------------------------
# Diagnose Command
#-------------------------------------------------------------------------------

cmd_diagnose() {
    print_section "Comprehensive Diagnostics"
    
    # System info
    echo -e "${CYAN}System Information:${NC}"
    echo "  Platform: ${OS_TYPE}"
    echo "  Hostname: $(hostname)"
    echo "  Date: $(date)"
    echo ""
    
    # Network interfaces
    echo -e "${CYAN}Network Interfaces:${NC}"
    if [[ "$OS_TYPE" == "macos" ]]; then
        ifconfig | grep -E "^[a-z]|inet " | head -20
    else
        ip -brief addr 2>/dev/null || ifconfig | grep -E "^[a-z]|inet " | head -20
    fi
    echo ""
    
    # Configuration check
    echo -e "${CYAN}Configuration:${NC}"
    if [[ -f "${CONFIG_FILE}" ]]; then
        print_success "Config file exists: ${CONFIG_FILE}"
        local server_addr=$(get_config_value "address")
        echo "  Server: ${server_addr}"
    else
        print_warning "Config file not found"
    fi
    echo ""
    
    # Binary check
    echo -e "${CYAN}Client Binary:${NC}"
    if [[ -f "${SOURCE_BINARY}" ]]; then
        print_success "Binary exists: ${SOURCE_BINARY}"
        echo "  Size: $(ls -lh "${SOURCE_BINARY}" | awk '{print $5}')"
        if [[ -x "${SOURCE_BINARY}" ]]; then
            echo "  Version: $("${SOURCE_BINARY}" --version 2>/dev/null | head -1)"
        fi
    else
        print_warning "Binary not found - run '${SCRIPT_NAME} build'"
    fi
    echo ""
    
    # DNS resolution
    if check_config_exists 2>/dev/null; then
        local host=$(get_server_host)
        local port=$(get_server_port)
        
        echo -e "${CYAN}Server Connectivity:${NC}"
        echo "  Host: ${host}"
        echo "  Port: ${port}"
        
        # Quick DNS check
        if cmd_exists dig; then
            local resolved=$(dig +short "$host" 2>/dev/null | head -1)
            if [[ -n "$resolved" ]]; then
                echo "  DNS: ${resolved}"
            else
                echo "  DNS: resolution failed"
            fi
        fi
        
        # Ping latency
        echo ""
        echo -e "${CYAN}Latency:${NC}"
        if cmd_exists ping; then
            print_step "Measuring latency to ${host}..."
            if [[ "$OS_TYPE" == "macos" ]]; then
                ping -c 3 -t 5 "$host" 2>/dev/null | tail -3
            else
                ping -c 3 -W 5 "$host" 2>/dev/null | tail -3
            fi
        fi
        echo ""
        
        # UDP connectivity hint
        echo -e "${CYAN}UDP/QUIC Notes:${NC}"
        echo "  - QUIC uses UDP (not TCP) for transport"
        echo "  - Some networks block or throttle UDP"
        echo "  - Corporate firewalls may block non-standard ports"
        echo "  - Try port 443 if other ports are blocked"
        echo ""
        
        # Check common issues
        echo -e "${CYAN}Common Issues:${NC}"
        
        # Check if port is in use locally
        local socks5_bind=$(get_config_value "socks5_bind")
        local http_bind=$(get_config_value "http_bind")
        
        local socks5_port="${socks5_bind##*:}"
        local http_port="${http_bind##*:}"
        
        if cmd_exists lsof; then
            if lsof -i ":${socks5_port}" &>/dev/null; then
                print_warning "Port ${socks5_port} is already in use"
            fi
            if lsof -i ":${http_port}" &>/dev/null; then
                print_warning "Port ${http_port} is already in use"
            fi
        fi
        
        # Check firewall on macOS
        if [[ "$OS_TYPE" == "macos" ]]; then
            local fw_status=$(/usr/libexec/ApplicationFirewall/socketfilterfw --getglobalstate 2>/dev/null | grep -o "enabled\|disabled")
            if [[ "$fw_status" == "enabled" ]]; then
                print_info "macOS Firewall is enabled"
            fi
        fi
        
        # Check common Linux firewall
        if [[ "$OS_TYPE" == "linux" ]]; then
            if cmd_exists ufw && ufw status 2>/dev/null | grep -q "active"; then
                print_info "UFW firewall is active"
            fi
            if cmd_exists firewall-cmd && firewall-cmd --state 2>/dev/null | grep -q "running"; then
                print_info "firewalld is running"
            fi
        fi
    fi
    
    print_section "Diagnostics Complete"
    print_info "Run '${SCRIPT_NAME} test' for full connectivity tests"
}

#-------------------------------------------------------------------------------
# Main Entry Point
#-------------------------------------------------------------------------------

# Detect platform first
detect_platform

print_header

if [[ $# -eq 0 ]]; then
    print_usage
    exit 0
fi

COMMAND="$1"
shift

case "$COMMAND" in
    build)
        cmd_build
        ;;
    setup)
        cmd_setup
        ;;
    config)
        cmd_config
        ;;
    test)
        cmd_test
        ;;
    test-dns)
        cmd_test_dns
        ;;
    test-port)
        cmd_test_port
        ;;
    test-tls)
        cmd_test_tls
        ;;
    test-quic)
        cmd_test_quic
        ;;
    test-proxy)
        cmd_test_proxy
        ;;
    diagnose)
        cmd_diagnose
        ;;
    -h|--help|help)
        print_usage
        ;;
    *)
        print_error "Unknown command: ${COMMAND}"
        echo ""
        print_usage
        exit 1
        ;;
esac

