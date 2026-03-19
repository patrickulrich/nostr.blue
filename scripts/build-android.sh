#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export ANDROID_GRADLE_VARIANT="${ANDROID_GRADLE_VARIANT:-${ANDROID_BUILD_MODE:-debug}}"
export ANDROID_RUST_PROFILE="${ANDROID_RUST_PROFILE:-release}"
export ANDROID_PACKAGE_FORMAT=apk

exec "$SCRIPT_DIR/android-build-common.sh"
