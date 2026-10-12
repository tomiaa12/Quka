#!/usr/bin/env bash
# 把自签名代码签名证书导入临时钥匙串，供 codesign 使用。
# 证书固定不变时，macOS 辅助功能授权在升级后依然有效。
set -euo pipefail

: "${MACOS_CERTIFICATE:?缺少 MACOS_CERTIFICATE（base64 的 .p12）}"
: "${MACOS_CERTIFICATE_PASSWORD:?缺少 MACOS_CERTIFICATE_PASSWORD}"

KEYCHAIN="${KEYCHAIN_PATH:-$RUNNER_TEMP/quka-signing.keychain-db}"
KEYCHAIN_PASSWORD="$(openssl rand -hex 16)"
CERT_FILE="$(mktemp -t quka-cert).p12"
trap 'rm -f "$CERT_FILE"' EXIT

printf '%s' "$MACOS_CERTIFICATE" | base64 --decode > "$CERT_FILE"

security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security set-keychain-settings -lut 21600 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security import "$CERT_FILE" -k "$KEYCHAIN" -P "$MACOS_CERTIFICATE_PASSWORD" \
  -f pkcs12 -T /usr/bin/codesign -T /usr/bin/security
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null

existing=$(security list-keychains -d user | tr -d '"' | xargs)
# shellcheck disable=SC2086
security list-keychains -d user -s "$KEYCHAIN" $existing

security find-identity -p codesigning "$KEYCHAIN"
