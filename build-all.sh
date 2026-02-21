#!/usr/bin/env bash
set -euo pipefail

APP="adbwrenchtui"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
OUT_DIR="dist"

# Targets: (rust-triple, friendly-name)
TARGETS=(
    "aarch64-apple-darwin:macos-arm64"
    "x86_64-apple-darwin:macos-x86_64"
    "x86_64-unknown-linux-gnu:linux-x86_64"
    "aarch64-unknown-linux-gnu:linux-arm64"
    "x86_64-pc-windows-gnu:windows-x86_64"
)

echo "=== Building ${APP} v${VERSION} ==="
echo ""

mkdir -p "${OUT_DIR}"

for entry in "${TARGETS[@]}"; do
    TARGET="${entry%%:*}"
    LABEL="${entry##*:}"
    EXT=""
    if [[ "${TARGET}" == *"windows"* ]]; then
        EXT=".exe"
    fi

    OUT_NAME="${APP}-v${VERSION}-${LABEL}${EXT}"
    echo "--- ${LABEL} (${TARGET}) ---"

    # Native macOS targets: use cargo directly (no Docker needed)
    if [[ "${TARGET}" == *"apple-darwin"* ]]; then
        rustup target add "${TARGET}" 2>/dev/null || true
        cargo build --release --target "${TARGET}"
        cp "target/${TARGET}/release/${APP}${EXT}" "${OUT_DIR}/${OUT_NAME}"
    else
        # Cross-compile via Docker
        cross build --release --target "${TARGET}"
        cp "target/${TARGET}/release/${APP}${EXT}" "${OUT_DIR}/${OUT_NAME}"
    fi

    echo "  -> ${OUT_DIR}/${OUT_NAME}"
    echo ""
done

# Generate checksums
echo "--- Generating checksums ---"
cd "${OUT_DIR}"
shasum -a 256 ${APP}-* > checksums-sha256.txt
cd ..
cat "${OUT_DIR}/checksums-sha256.txt"
echo ""

echo "=== All builds complete ==="
ls -lh "${OUT_DIR}/"
