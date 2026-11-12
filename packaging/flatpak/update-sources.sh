#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

if ! command -v uv >/dev/null 2>&1 || ! command -v uvx >/dev/null 2>&1; then
  echo "Erro: instale 'uv' (https://docs.astral.sh/uv/) para regenerar as fontes offline do Flatpak." >&2
  exit 1
fi

git clone --depth 1 https://github.com/flatpak/flatpak-builder-tools.git "${TMP_DIR}/flatpak-builder-tools"

# flatpak-node-generator exige executar fora de um diretório com node_modules presente
cp "${REPO_ROOT}/package-lock.json" "${TMP_DIR}/package-lock.json"

uv run "${TMP_DIR}/flatpak-builder-tools/cargo/flatpak-cargo-generator.py" \
  "${REPO_ROOT}/src-tauri/Cargo.lock" \
  -o "${SCRIPT_DIR}/cargo-sources.json"

uvx --from "${TMP_DIR}/flatpak-builder-tools/node" \
  flatpak-node-generator npm "${TMP_DIR}/package-lock.json" \
  -o "${SCRIPT_DIR}/node-sources.json"

echo "Fontes atualizadas em ${SCRIPT_DIR}/cargo-sources.json e ${SCRIPT_DIR}/node-sources.json."
