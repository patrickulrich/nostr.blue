#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export ANDROID_BUILD_MODE="${ANDROID_BUILD_MODE:-release}"
export ANDROID_PACKAGE_FORMAT=aab

exec "$SCRIPT_DIR/android-build-common.sh"
