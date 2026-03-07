#!/bin/bash
set -e

require_file() {
    local path="$1"
    local message="$2"
    if [ ! -f "$path" ]; then
        echo "ERROR: $message: $path" >&2
        exit 1
    fi
}

version_field() {
    local field="$1"
    sed -n "s/^$field = \"\\([^\"]*\\)\"$/\\1/p" "$PROJECT_ROOT/Cargo.toml" | head -n1
}

version_code_from_semver() {
    local version="$1"
    IFS=. read -r major minor patch <<EOF
$version
EOF
    if [ -z "$major" ] || [ -z "$minor" ] || [ -z "$patch" ]; then
        echo "ERROR: Unsupported Cargo.toml version format: $version" >&2
        exit 1
    fi
    echo $((major * 10000 + minor * 100 + patch))
}

verify_gradle_value() {
    local label="$1"
    local expected="$2"
    local pattern="$3"
    local actual
    actual=$(sed -n "s/$pattern/\\1/p" "$GRADLE_APP" | head -n1)
    if [ "$actual" != "$expected" ]; then
        echo "ERROR: $label mismatch in generated Gradle config. Expected '$expected', found '$actual'" >&2
        exit 1
    fi
}

# Android SDK/NDK paths
ANDROID_HOME="${ANDROID_HOME:-${HOME}/Android/Sdk}"
if [ -z "$ANDROID_NDK_HOME" ]; then
    if [ -d "$ANDROID_HOME/ndk" ]; then
        # Use find to robustly handle directories with special characters
        NDK_VERSION=$(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; 2>/dev/null | sort -V | tail -n1)
        if [ -n "$NDK_VERSION" ]; then
            ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$NDK_VERSION"
        else
            ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
        fi
    else
        ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
    fi
fi
ANDROID_SDK_ROOT="$ANDROID_HOME"
if [ ! -d "$ANDROID_NDK_HOME" ]; then
    echo "ERROR: ANDROID_NDK_HOME does not exist: $ANDROID_NDK_HOME" >&2
    echo "  Install NDK via: sdkmanager --install 'ndk;27.0.12077973'" >&2
    exit 1
fi
export ANDROID_HOME ANDROID_NDK_HOME ANDROID_SDK_ROOT

# Project paths
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DX_ANDROID="$PROJECT_ROOT/target/dx/nostrblue/release/android/app"
ANDROID_RES_SRC="$PROJECT_ROOT/android/res"
ANDROID_KOTLIN_SRC="$PROJECT_ROOT/android/kotlin"
APP_ID="com.nostr.blue"
CARGO_VERSION="$(version_field version)"
ANDROID_VERSION_CODE="$(version_code_from_semver "$CARGO_VERSION")"
GRADLE_APP="$DX_ANDROID/app/build.gradle.kts"
GENERATED_MAIN_ACTIVITY="$DX_ANDROID/app/src/main/kotlin/dev/dioxus/main/MainActivity.kt"
GRADLE_USER_HOME="${GRADLE_USER_HOME:-$PROJECT_ROOT/.gradle-home}"

mkdir -p "$GRADLE_USER_HOME"
export GRADLE_USER_HOME

require_file "$ANDROID_RES_SRC/mipmap-anydpi-v26/ic_launcher.xml" "Adaptive launcher XML not found"
require_file "$ANDROID_RES_SRC/mipmap-xxxhdpi/ic_launcher.webp" "Launcher icon asset not found"
require_file "$ANDROID_RES_SRC/mipmap-xxxhdpi/ic_launcher_foreground.png" "Launcher foreground asset not found"
require_file "$ANDROID_RES_SRC/drawable/ic_launcher_background.xml" "Launcher background asset not found"
require_file "$ANDROID_KOTLIN_SRC/dev/dioxus/main/MediaPlaybackService.kt" "Native playback service source not found"
require_file "$ANDROID_KOTLIN_SRC/dev/dioxus/main/NativeAudioBridge.kt" "Native audio bridge source not found"

echo "=== nostr.blue Android Build ==="
echo "Project: $PROJECT_ROOT"
echo "NDK: $ANDROID_NDK_HOME"
echo "Version: $CARGO_VERSION ($ANDROID_VERSION_CODE)"
echo "Gradle home: $GRADLE_USER_HOME"
echo "Android resources: $ANDROID_RES_SRC"

# 1. Clean stale architecture artifacts
echo ""
echo "--- Step 1: Clean stale jniLibs ---"
if [ -d "$DX_ANDROID/app/src/main/jniLibs" ]; then
    rm -rf "$DX_ANDROID/app/src/main/jniLibs"
    echo "Cleaned stale jniLibs"
else
    echo "No stale jniLibs to clean"
fi

echo "Cleaning stale generated launcher overrides"
find "$DX_ANDROID" \
    \( -path '*/mipmap-*/ic_launcher.png' \
    -o -path '*/mipmap-*/ic_launcher_round.png' \
    -o -path '*/mipmap-*/ic_launcher_round.webp' \
    -o -path '*/mipmap-*/ic_launcher_foreground.png' \
    -o -path '*/mipmap-anydpi-v26/ic_launcher_round.xml' \) \
    -delete 2>/dev/null || true

# 1a. Pre-copy Android resources (before dx build runs Gradle)
echo ""
echo "--- Step 1a: Pre-copy Android resources ---"
mkdir -p "$PROJECT_ROOT/target/dx/nostrblue/release/android/app/app/src/main/res/xml"
cp "$PROJECT_ROOT/android/res/xml/file_paths.xml" "$PROJECT_ROOT/target/dx/nostrblue/release/android/app/app/src/main/res/xml/" 2>/dev/null && echo "Pre-copied file_paths.xml" || echo "Directory not yet created (will be handled post-build)"
mkdir -p "$PROJECT_ROOT/target/dx/nostrblue/release/android/app/app/src/main/kotlin/dev/dioxus/main"
cp "$ANDROID_KOTLIN_SRC/dev/dioxus/main/"*.kt "$PROJECT_ROOT/target/dx/nostrblue/release/android/app/app/src/main/kotlin/dev/dioxus/main/" 2>/dev/null && echo "Pre-copied Android Kotlin sources" || echo "Kotlin source directory not yet created (will be handled post-build)"

# 2. Build (generates Android project + compiles Rust + runs Gradle)
echo ""
echo "--- Step 2: dx build (ARM64) ---"
cd "$PROJECT_ROOT"
dx build --platform android --release --target aarch64-linux-android --no-default-features --features mobile

# 2b. Clean non-Android files from generated project
echo ""
echo "--- Step 2b: Clean non-Android files ---"
find "$DX_ANDROID" -name "CLAUDE.md" -type f -delete 2>/dev/null && echo "Cleaned CLAUDE.md files" || echo "No CLAUDE.md files to clean"

# 2b.i. Normalize Android package metadata in generated Gradle config
echo ""
echo "--- Step 2b.i: Normalize Android metadata ---"
require_file "$GRADLE_APP" "Generated Android Gradle config not found"
require_file "$GENERATED_MAIN_ACTIVITY" "Generated Android MainActivity not found"
python3 - "$GRADLE_APP" "$APP_ID" "$CARGO_VERSION" "$ANDROID_VERSION_CODE" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
app_id = sys.argv[2]
version_name = sys.argv[3]
version_code = sys.argv[4]
content = path.read_text()
replacements = [
    (r'namespace="[^"]*"', f'namespace="{app_id}"'),
    (r'applicationId = "[^"]*"', f'applicationId = "{app_id}"'),
    (r'versionName = "[^"]*"', f'versionName = "{version_name}"'),
    (r'versionCode = \d+', f'versionCode = {version_code}'),
]
for pattern, replacement in replacements:
    content, count = re.subn(pattern, replacement, content, count=1)
    if count != 1:
        raise SystemExit(f"failed to patch {pattern} in {path}")
path.write_text(content)
PY
python3 - "$GENERATED_MAIN_ACTIVITY" "$APP_ID" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
app_id = sys.argv[2]
content = path.read_text()
content, count = re.subn(
    r'typealias BuildConfig = [A-Za-z0-9_.]+\.BuildConfig',
    f'typealias BuildConfig = {app_id}.BuildConfig',
    content,
    count=1,
)
if count != 1:
    raise SystemExit(f"failed to patch BuildConfig alias in {path}")
path.write_text(content)
PY
verify_gradle_value "applicationId" "$APP_ID" '^[[:space:]]*applicationId = "\([^\"]*\)"$'
verify_gradle_value "namespace" "$APP_ID" '^[[:space:]]*namespace="\([^\"]*\)"$'
verify_gradle_value "versionName" "$CARGO_VERSION" '^[[:space:]]*versionName = "\([^\"]*\)"$'
verify_gradle_value "versionCode" "$ANDROID_VERSION_CODE" '^[[:space:]]*versionCode = \([0-9][0-9]*\)$'
echo "Normalized Gradle metadata for $APP_ID"

# 2c. Ensure OpenSSL shared libs are in jniLibs
# (dx CLI copies these from link_args, but cargo caching can cause
# the linker wrapper to not run, leaving link_args empty)
echo ""
echo "--- Step 2c: Ensure OpenSSL libs ---"
OPENSSL_SEARCH="$HOME/.local/share/.dx/prebuilt"
OPENSSL_PREBUILT=""
if [ -d "$OPENSSL_SEARCH" ]; then
    matches=()
    for dir in "$OPENSSL_SEARCH"/openssl*/ssl/libs/android.arm64-v8a; do
        if [ -f "$dir/libssl.so" ] && [ -f "$dir/libcrypto.so" ]; then
            matches+=("$dir")
        fi
    done
    if [ ${#matches[@]} -gt 0 ]; then
        # Sort by modification time (newest first)
        sorted=()
        while IFS= read -r line; do
            sorted+=("$line")
        done < <(for m in "${matches[@]}"; do
            mtime=$(stat -c %Y "$m" 2>/dev/null || echo 0)
            echo "$mtime $m"
        done | sort -rn | cut -d' ' -f2-)
        OPENSSL_PREBUILT="${sorted[0]}"
    fi
fi
if [ -z "$OPENSSL_PREBUILT" ]; then
    echo "ERROR: No OpenSSL prebuilt with libssl.so and libcrypto.so found in $OPENSSL_SEARCH"
    echo "  Run 'dx build --platform android' once to extract prebuilt libs"
    exit 1
fi
echo "Found OpenSSL prebuilt at: $OPENSSL_PREBUILT"
JNILIBS="$DX_ANDROID/app/src/main/jniLibs/arm64-v8a"

if [ ! -f "$JNILIBS/libssl.so" ] || [ ! -f "$JNILIBS/libcrypto.so" ]; then
    if [ -f "$OPENSSL_PREBUILT/libssl.so" ] && [ -f "$OPENSSL_PREBUILT/libcrypto.so" ]; then
        mkdir -p "$JNILIBS"
        cp "$OPENSSL_PREBUILT/libssl.so" "$JNILIBS/"
        cp "$OPENSSL_PREBUILT/libcrypto.so" "$JNILIBS/"
        echo "Copied OpenSSL libs to jniLibs (from Dioxus prebuilt)"
    else
        echo "ERROR: Prebuilt OpenSSL not found at $OPENSSL_PREBUILT"
        echo "  Run 'dx build --platform android' once to extract prebuilt libs"
        exit 1
    fi
else
    echo "OpenSSL libs already present in jniLibs (dx CLI copied them)"
fi

# 3. Copy custom ProGuard rules (for release builds with R8 minification)
echo ""
echo "--- Step 3: Copy ProGuard rules ---"
PROGUARD_SRC="$PROJECT_ROOT/android/proguard-rules.pro"
PROGUARD_DST="$DX_ANDROID/app/proguard-rules.pro"
if [ -f "$PROGUARD_SRC" ]; then
    cp "$PROGUARD_SRC" "$PROGUARD_DST"
    echo "Copied custom proguard-rules.pro (JNI keep rules)"
else
    echo "WARNING: $PROGUARD_SRC not found, skipping ProGuard rules"
fi

# 4. Overlay repo-owned Android resources
echo ""
echo "--- Step 4: Overlay Android resources ---"
if [ -d "$ANDROID_RES_SRC" ]; then
    find "$DX_ANDROID/app/src/main/res" \
        \( -path '*/mipmap-*/ic_launcher.webp' \
        -o -path '*/mipmap-*/ic_launcher_round.webp' \
        -o -path '*/mipmap-*/ic_launcher_foreground.webp' \
        -o -path '*/mipmap-*/ic_launcher_foreground.png' \
        -o -path '*/mipmap-anydpi-v26/ic_launcher.xml' \
        -o -path '*/mipmap-anydpi-v26/ic_launcher_round.xml' \
        -o -path '*/drawable-v24/ic_launcher_foreground.xml' \
        -o -path '*/drawable/ic_launcher_background.xml' \) \
        -delete
    cp -R "$ANDROID_RES_SRC/." "$DX_ANDROID/app/src/main/res/"
    echo "Copied repo-owned Android resources from android/res"
else
    echo "WARNING: $ANDROID_RES_SRC not found, skipping Android resource overlay"
fi

echo ""
echo "--- Step 4c: Copy Android Kotlin sources ---"
mkdir -p "$DX_ANDROID/app/src/main/kotlin/dev/dioxus/main"
cp "$ANDROID_KOTLIN_SRC/dev/dioxus/main/"*.kt "$DX_ANDROID/app/src/main/kotlin/dev/dioxus/main/"
echo "Copied native Android Kotlin sources"

# 4d. Verify critical resource overrides
echo ""
echo "--- Step 4d: Verify Android resource overrides ---"
require_file "$DX_ANDROID/app/src/main/res/values/strings.xml" "Missing app strings.xml in generated project"
require_file "$DX_ANDROID/app/src/main/res/xml/file_paths.xml" "Missing file_paths.xml in generated project"
require_file "$DX_ANDROID/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml" "Missing adaptive launcher icon"
require_file "$DX_ANDROID/app/src/main/res/mipmap-xxxhdpi/ic_launcher.webp" "Missing launcher icon density asset"
require_file "$DX_ANDROID/app/src/main/res/mipmap-xxxhdpi/ic_launcher_foreground.png" "Missing launcher foreground density asset"

# 5. Re-run Gradle to pick up icon/name/proguard changes
echo ""
echo "--- Step 5: Re-run Gradle ---"
cd "$DX_ANDROID"
./gradlew assembleDebug

# 6. Copy output APK
echo ""
echo "--- Step 6: Copy APK ---"
APK_SRC="$DX_ANDROID/app/build/outputs/apk/debug/app-debug.apk"
APK_DST="$PROJECT_ROOT/nostrblue-debug.apk"
if [ -f "$APK_SRC" ]; then
    cp "$APK_SRC" "$APK_DST"
    echo "APK: $APK_DST"
    # Verify ARM64 only
    if command -v unzip &>/dev/null; then
        echo ""
        echo "Native libs in APK:"
        unzip -l "$APK_DST" "lib/*" 2>/dev/null | grep "\.so" | awk '{print $NF}' || true
    fi
else
    echo "ERROR: APK not found at $APK_SRC"
    exit 1
fi

echo ""
echo "=== Build complete ==="
