#!/bin/sh
set -eu

REPO="home-still/convodump"
TOOL="convodump"
INSTALL_DIR="${HOME}/.local/bin"

if [ "$#" -ne 0 ]; then
    echo "Usage: install.sh"
    exit 1
fi

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}-${ARCH}" in
    Darwin-arm64)                  TARGET="aarch64-apple-darwin" ;;
    Linux-x86_64)                  TARGET="x86_64-unknown-linux-gnu" ;;
    *)  echo "Unsupported platform: ${OS} ${ARCH} (supported: macOS arm64, Linux x86_64)"; exit 1 ;;
esac

VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')"
if [ -z "${VERSION}" ]; then
    echo "Failed to fetch latest version"
    exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
    sha256_of() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
    sha256_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
    echo "Neither sha256sum nor shasum found; cannot verify the download"
    exit 1
fi

mkdir -p "${INSTALL_DIR}"
# Staged inside INSTALL_DIR so the final mv is a same-filesystem rename: a
# running binary is replaced atomically, and a failed download or checksum
# leaves the previously installed binary untouched.
WORK="$(mktemp -d "${INSTALL_DIR}/.install.XXXXXX")"
trap 'rm -rf "${WORK}"' EXIT

archive="${TOOL}-${VERSION}-${TARGET}.tar.gz"
base="https://github.com/${REPO}/releases/download/${VERSION}/${archive}"

echo "Installing ${TOOL} ${VERSION} for ${TARGET}..."

code="$(curl -sSL -o "${WORK}/${archive}" -w '%{http_code}' "${base}")" \
    || { echo "Download of ${archive} failed"; exit 1; }
[ "${code}" = 200 ] || { echo "Download of ${archive} failed: HTTP ${code}"; exit 1; }

code="$(curl -sSL -o "${WORK}/${archive}.sha256" -w '%{http_code}' "${base}.sha256")" \
    || { echo "Download of ${archive}.sha256 failed"; exit 1; }
[ "${code}" = 200 ] || { echo "Download of ${archive}.sha256 failed: HTTP ${code}"; exit 1; }

# `<64 hex>  <archive name>`
expected="$(awk -v name="${archive}" '{ sub(/\r$/, "") } NF == 2 && $2 == name { print $1 }' "${WORK}/${archive}.sha256")"
actual="$(sha256_of "${WORK}/${archive}")"
if [ -z "${expected}" ] || [ "${expected}" != "${actual}" ]; then
    echo "Checksum mismatch for ${archive}: expected '${expected}', got '${actual}'"
    exit 1
fi

tar -xzf "${WORK}/${archive}" -C "${WORK}" "${TOOL}"
chmod +x "${WORK}/${TOOL}"
mv -f "${WORK}/${TOOL}" "${INSTALL_DIR}/${TOOL}"
echo "Installed ${TOOL} to ${INSTALL_DIR}/${TOOL}"

case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
        echo ""
        echo "Add ${INSTALL_DIR} to your PATH:"
        echo "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.bashrc"
        ;;
esac
