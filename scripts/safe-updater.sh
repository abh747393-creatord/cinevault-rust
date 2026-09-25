#!/usr/bin/env bash
# CineVault Safe MovieBox-TUI Host Updater (POSIX / Linux)
# Safely pins, compiles, and verifies MovieBox-TUI dependency updates on the authorized backend host.

set -euo pipefail

TARGET_VERSION="${1:-}"
MODE="${2:-}"

if [ -z "$TARGET_VERSION" ]; then
    echo "Usage: $0 <target_version> [--dry-run|--restart]"
    exit 1
fi

CLEAN_VERSION="${TARGET_VERSION#v}"
echo "=================================================="
echo " CineVault Safe MovieBox-TUI Host Updater"
echo "=================================================="
echo "Target Version: v${CLEAN_VERSION}"

# 1. Validate version syntax (Strict stable semver: digits and dots only)
if ! [[ "$CLEAN_VERSION" =~ ^[0-9]{1,4}\.[0-9]{1,4}\.[0-9]{1,4}$ ]]; then
    echo "Error: Invalid semantic version format: '$TARGET_VERSION'. Only stable semver (e.g. 0.1.24) is permitted."
    exit 1
fi

# 2. Verify with GitHub API (official stable releases only)
echo "[1/6] Verifying stable release tag with GitHub API..."
API_URL="https://api.github.com/repos/mesamirh/MovieBox-Tui/releases?per_page=15"
TAGS=$(curl -sSL -H "User-Agent: CineVault-Host-Updater" "$API_URL" | grep -o '"tag_name": *"[^"]*"' | tr -d '"' | awk '{print $2}' || true)

FOUND=0
for tag in $TAGS; do
    if [ "${tag#v}" = "$CLEAN_VERSION" ]; then
        FOUND=1
        break
    fi
done

if [ "$FOUND" -ne 1 ]; then
    echo "Error: Target version '$CLEAN_VERSION' not found in upstream stable releases."
    exit 1
fi
echo "  -> Verified upstream stable release tag."

if [ "$MODE" = "--dry-run" ]; then
    echo ""
    echo "[DRY RUN COMPLETE] Target version v${CLEAN_VERSION} is valid and verified against GitHub."
    echo "Pre-flight checks passed successfully. No files were modified."
    exit 0
fi

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="${REPO_DIR}/Cargo.toml"
CARGO_LOCK="${REPO_DIR}/Cargo.lock"
CARGO_TOML_BAK="${CARGO_TOML}.bak"
CARGO_LOCK_BAK="${CARGO_LOCK}.bak"
LOCK_FILE="${REPO_DIR}/.updater.lock"

if [ ! -f "$CARGO_TOML" ]; then
    echo "Error: Cargo.toml not found in $REPO_DIR"
    exit 1
fi

# Concurrency check
if [ -f "$LOCK_FILE" ]; then
    echo "Error: Another update process is currently running ($LOCK_FILE exists). Aborting."
    exit 1
fi
touch "$LOCK_FILE"

# 3. Create atomic backups
echo "[2/6] Creating atomic configuration backups..."
cp "$CARGO_TOML" "$CARGO_TOML_BAK"
[ -f "$CARGO_LOCK" ] && cp "$CARGO_LOCK" "$CARGO_LOCK_BAK"

cleanup_and_rollback() {
    echo ""
    echo "[ERROR] Update failed! Performing atomic rollback..."
    [ -f "$CARGO_TOML_BAK" ] && cp "$CARGO_TOML_BAK" "$CARGO_TOML" && rm -f "$CARGO_TOML_BAK"
    [ -f "$CARGO_LOCK_BAK" ] && cp "$CARGO_LOCK_BAK" "$CARGO_LOCK" && rm -f "$CARGO_LOCK_BAK"
    rm -f "$LOCK_FILE"
    echo "Rollback complete. Restored original working version."
}

trap cleanup_and_rollback ERR

# 4. Pin version in Cargo.toml
echo "[3/6] Pinning version in Cargo.toml..."
sed -i.tmp -E "s/(name *= *\"moviebox-tui\"[^\"]*version *= *\")[^\"]+(\")/\1${CLEAN_VERSION}\2/" "$CARGO_TOML"
rm -f "${CARGO_TOML}.tmp"

# 5. Build and Test Verification
echo "[4/6] Running cargo check --bin cinevault_server..."
(cd "$REPO_DIR" && cargo check --bin cinevault_server)

echo "[5/6] Running cargo test --bin cinevault_server..."
(cd "$REPO_DIR" && cargo test --bin cinevault_server)

# 6. Optional: Safe Stop, Compile Binary, Restart & /health Verification
if [ "$MODE" = "--restart" ]; then
    echo "[6/6] Safe stop, compile, restart, and /health verification..."
    pkill -f cinevault_server || true
    (cd "$REPO_DIR" && cargo build --bin cinevault_server)
    (PORT=8080 nohup "${REPO_DIR}/target/debug/cinevault_server" > /dev/null 2>&1 &)
    sleep 2
    HEALTH_STATUS=$(curl -s "http://127.0.0.1:8080/health" | grep -o '"status":"healthy"' || true)
    if [ -z "$HEALTH_STATUS" ]; then
        echo "Error: Live /health check failed."
        false
    fi
    echo "  -> /health verified healthy."
fi

# Success: Clean up backups and lockfile
trap - ERR
rm -f "$CARGO_TOML_BAK" "$CARGO_LOCK_BAK" "$LOCK_FILE"

echo ""
echo "=================================================="
echo " UPDATE SUCCESSFULLY APPLIED AND VERIFIED!"
echo "=================================================="
echo "Version is now pinned to v${CLEAN_VERSION}."
