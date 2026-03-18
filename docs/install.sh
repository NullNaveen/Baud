#!/usr/bin/env bash
# Baud — One-Command Linux/macOS/WSL Installer
# Run: curl -sSL https://nullnaveen.github.io/Baud/install.sh | bash
#
# This script:
#   1. Checks for Git and Rust (installs Rust if missing)
#   2. Clones the Baud repository
#   3. Builds baud-node from source
#   4. Generates a unique validator secret key
#   5. Creates a launch script
#   6. Prints your address and secret key

set -e

BAUD_DIR="$HOME/baud"
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo ""
echo -e "${CYAN}  ========================================${NC}"
echo -e "${CYAN}    BAUD — M2M Agent Ledger Installer${NC}"
echo -e "${CYAN}  ========================================${NC}"
echo ""

# --- Check Git ---
if ! command -v git &>/dev/null; then
    echo -e "${RED}[ERROR] Git is not installed.${NC}"
    echo "  Ubuntu/Debian: sudo apt install git"
    echo "  macOS: xcode-select --install"
    exit 1
fi
echo -e "${GREEN}[OK] Git found: $(git --version)${NC}"

# --- Check / Install Rust ---
if ! command -v rustc &>/dev/null; then
    echo -e "${YELLOW}[...] Rust not found. Installing via rustup...${NC}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi
echo -e "${GREEN}[OK] Rust found: $(rustc --version)${NC}"

# --- Install build deps (Linux) ---
if command -v apt-get &>/dev/null; then
    if ! dpkg -s build-essential &>/dev/null 2>&1 || ! dpkg -s pkg-config &>/dev/null 2>&1 || ! dpkg -s libssl-dev &>/dev/null 2>&1; then
        echo -e "${YELLOW}[...] Installing build dependencies...${NC}"
        sudo apt-get update -qq
        sudo apt-get install -y -qq build-essential pkg-config libssl-dev
    fi
fi

# --- Clone or update repo ---
if [ -d "$BAUD_DIR/.git" ]; then
    echo -e "${YELLOW}[...] Updating existing Baud installation...${NC}"
    cd "$BAUD_DIR"
    git pull origin main --ff-only 2>/dev/null || true
else
    echo -e "${YELLOW}[...] Cloning Baud repository...${NC}"
    git clone https://github.com/NullNaveen/Baud.git "$BAUD_DIR"
fi
echo -e "${GREEN}[OK] Source code ready at $BAUD_DIR${NC}"

# --- Build ---
echo -e "${YELLOW}[...] Building baud-node (this takes 1-3 minutes)...${NC}"
cd "$BAUD_DIR"
cargo build --bin baud-node --release 2>&1 | tail -1
if [ ! -f "target/release/baud-node" ]; then
    echo -e "${RED}[ERROR] Build failed.${NC}"
    exit 1
fi
echo -e "${GREEN}[OK] baud-node built successfully${NC}"

# --- Generate keypair ---
echo -e "${YELLOW}[...] Generating your validator keypair...${NC}"
SECRET_KEY=$(openssl rand -hex 32)

# Save key
KEY_FILE="$BAUD_DIR/my-secret-key.txt"
echo "# BAUD VALIDATOR SECRET KEY — KEEP THIS SAFE! NEVER SHARE IT!" > "$KEY_FILE"
echo "# Generated: $(date -Iseconds)" >> "$KEY_FILE"
echo "$SECRET_KEY" >> "$KEY_FILE"
chmod 600 "$KEY_FILE"
echo -e "${GREEN}[OK] Secret key saved to $KEY_FILE${NC}"

# --- Create launch script ---
cat > "$BAUD_DIR/start-node.sh" << SCRIPT
#!/usr/bin/env bash
cd "$BAUD_DIR"
echo "Starting Baud node..."
echo "Dashboard: http://localhost:8080"
echo "Press Ctrl+C to stop."
echo ""
./target/release/baud-node --secret-key $SECRET_KEY
SCRIPT
chmod +x "$BAUD_DIR/start-node.sh"

# --- Create stop script ---
cat > "$BAUD_DIR/stop-node.sh" << 'SCRIPT'
#!/usr/bin/env bash
pkill -f baud-node && echo "Baud node stopped." || echo "No Baud node running."
SCRIPT
chmod +x "$BAUD_DIR/stop-node.sh"

# --- Add to PATH (optional) ---
if ! echo "$PATH" | grep -q "$BAUD_DIR/target/release"; then
    SHELL_RC="$HOME/.bashrc"
    [ -f "$HOME/.zshrc" ] && SHELL_RC="$HOME/.zshrc"
    echo "" >> "$SHELL_RC"
    echo "# Baud" >> "$SHELL_RC"
    echo "export PATH=\"$BAUD_DIR/target/release:\$PATH\"" >> "$SHELL_RC"
    echo -e "${GREEN}[OK] Added baud-node to PATH in $SHELL_RC${NC}"
fi

# --- Done! ---
echo ""
echo -e "${GREEN}  ========================================${NC}"
echo -e "${GREEN}    BAUD INSTALLED SUCCESSFULLY!${NC}"
echo -e "${GREEN}  ========================================${NC}"
echo ""
echo -e "  Install location:  $BAUD_DIR"
echo -e "  Secret key file:   $KEY_FILE"
echo -e "  Secret key:        ${YELLOW}$SECRET_KEY${NC}"
echo ""
echo -e "${CYAN}  TO START MINING:${NC}"
echo -e "    cd ~/baud && ./start-node.sh"
echo ""
echo -e "  TO STOP:"
echo -e "    ./stop-node.sh  (or Ctrl+C)"
echo ""
echo -e "  DASHBOARD:"
echo -e "    Open http://localhost:8080 in your browser"
echo ""
echo -e "  ${RED}IMPORTANT: Back up your secret key! If lost, your BAUD is gone forever.${NC}"
echo ""
