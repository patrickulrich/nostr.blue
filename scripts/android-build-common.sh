#!/bin/bash
set -euo pipefail

if ! command -v python3 >/dev/null 2>&1; then
    echo "ERROR: python3 is required but not found; please install Python 3" >&2
    exit 1
fi

# Keep this fallback in sync with ANDROID_NDK_HOME/ANDROID_SDK_ROOT auto-discovery
# and any documented NDK_VERSION setup in project docs.
NDK_FALLBACK_VERSION="${NDK_FALLBACK_VERSION:-27.0.12077973}"

require_file() {
    local path="$1"
    local message="$2"
    if [ ! -f "$path" ]; then
        echo "ERROR: $message: $path" >&2
        exit 1
    fi
}

require_files() {
    local missing=()
    if [ $(( $# % 2 )) -ne 0 ]; then
        echo "ERROR: require_files expects path/label argument pairs" >&2
        exit 1
    fi

    while [ "$#" -gt 1 ]; do
        local path="$1"
        local label="$2"
        shift 2
        if [ ! -f "$path" ]; then
            missing+=("$label: $path")
        fi
    done

    if [ "${#missing[@]}" -gt 0 ]; then
        echo "ERROR: Missing required Android launcher assets:" >&2
        for entry in "${missing[@]}"; do
            echo "  - $entry" >&2
        done
        exit 1
    fi
}

sync_overlay_dir() {
    local src_dir="$1"
    local dest_dir="$2"
    shift 2

    mkdir -p "$dest_dir"

    if command -v rsync >/dev/null 2>&1; then
        local rsync_args=(--archive --delete)
        local preserve
        for preserve in "$@"; do
            rsync_args+=(--filter="P $preserve")
        done
        rsync "${rsync_args[@]}" "$src_dir"/ "$dest_dir"/
        return
    fi

    local path rel preserve skip
    while IFS= read -r -d '' path; do
        rel="${path#"$dest_dir"/}"
        skip=0
        for preserve in "$@"; do
            if [ "$rel" = "$preserve" ]; then
                skip=1
                break
            fi
        done
        if [ "$skip" -eq 1 ]; then
            continue
        fi
        if [ ! -e "$src_dir/$rel" ]; then
            rm -rf "$path"
        fi
    done < <(find "$dest_dir" -mindepth 1 -depth -print0 2>/dev/null)

    while IFS= read -r -d '' path; do
        rel="${path#"$src_dir"/}"
        if [ -d "$path" ]; then
            mkdir -p "$dest_dir/$rel"
        else
            mkdir -p "$dest_dir/$(dirname "$rel")"
            cp "$path" "$dest_dir/$rel"
        fi
    done < <(find "$src_dir" -mindepth 1 -print0)
}

version_field() {
    local field="$1"
    awk -v field="$field" '
        /^\[package\]$/ { in_package = 1; next }
        /^\[/ && in_package { exit }
        in_package && $0 ~ ("^" field " = \"") {
            sub("^" field " = \"", "")
            sub("\"$", "")
            print
            exit
        }
    ' "$PROJECT_ROOT/Cargo.toml"
}

version_code_from_semver() {
    local version="$1"
    version="${version%%+*}"
    version="${version%%-*}"
    IFS=. read -r major minor patch <<EOF
$version
EOF
    if [ -z "$major" ] || [ -z "$minor" ] || [ -z "$patch" ]; then
        echo "ERROR: Unsupported Cargo.toml version format: $version" >&2
        exit 1
    fi

    if ! [[ "$major" =~ ^[0-9]+$ && "$minor" =~ ^[0-9]+$ && "$patch" =~ ^[0-9]+$ ]]; then
        echo "ERROR: version_code_from_semver requires numeric major.minor.patch segments, got: $version" >&2
        exit 1
    fi

    if [ "$minor" -ge 100 ] || [ "$patch" -ge 100 ]; then
        echo "ERROR: version_code_from_semver requires minor and patch to be less than 100, got: $version" >&2
        exit 1
    fi

    local version_code=$((major * 10000 + minor * 100 + patch))
    if [ "$version_code" -gt 2147483647 ]; then
        echo "ERROR: version_code_from_semver exceeds Android versionCode max (2147483647): $version -> $version_code" >&2
        exit 1
    fi

    echo "$version_code"
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

write_android_local_properties() {
    local local_properties="$DX_ANDROID/local.properties"
    local sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-${HOME}/Android/Sdk}}"
    local ndk_dir=""

    if [ -n "${ANDROID_NDK_HOME:-}" ]; then
        ndk_dir="$ANDROID_NDK_HOME"
    elif [ -d "$sdk_root/ndk" ]; then
        ndk_dir="$sdk_root/ndk"
    fi

    mkdir -p "$DX_ANDROID"
    {
        printf 'sdk.dir=%s\n' "$(printf '%s' "$sdk_root" | sed 's/\\/\\\\/g')"
        printf 'ndk.dir=%s\n' "$(printf '%s' "$ndk_dir" | sed 's/\\/\\\\/g')"
    } >"$local_properties"
    echo "Wrote Android SDK config: $local_properties"
}

normalize_gradle_properties() {
    local gradle_properties="$DX_ANDROID/gradle.properties"
    [ -f "$gradle_properties" ] || return
    python3 - "$gradle_properties" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
lines = path.read_text().splitlines()
filtered = [
    line for line in lines
    if line.strip() != "android.defaults.buildfeatures.buildconfig=true"
]

for line in ["android.javaCompile.suppressSourceTargetDeprecationWarning=true"]:
    if line not in filtered:
        filtered.append(line)

path.write_text("\n".join(filtered) + "\n")
PY
    echo "Normalized Gradle properties"
}

require_env() {
    local name="$1"
    if [ -z "${!name:-}" ]; then
        echo "ERROR: Required environment variable is missing: $name" >&2
        exit 1
    fi
}

cleanup() {
    if [ -n "${DIOXUS_CONFIG_BACKUP:-}" ] && [ -f "${DIOXUS_CONFIG_BACKUP}" ]; then
        mv "${DIOXUS_CONFIG_BACKUP}" "$DIOXUS_CONFIG"
    fi
}

configure_release_signing() {
    if grep -q '^\[bundle\.android\]' "$DIOXUS_CONFIG"; then
        echo "INFO: Dioxus.toml already defines [bundle.android]; using existing release signing config"
        return 0
    fi

    require_env ANDROID_KEYSTORE_FILE
    if [ ! -f "$ANDROID_KEYSTORE_FILE" ] || [ ! -r "$ANDROID_KEYSTORE_FILE" ]; then
        echo "ERROR: ANDROID_KEYSTORE_FILE is not a readable regular file: $ANDROID_KEYSTORE_FILE" >&2
        exit 1
    fi
    require_env ANDROID_KEYSTORE_PASSWORD
    require_env ANDROID_KEY_ALIAS
    require_env ANDROID_KEY_PASSWORD

    DIOXUS_CONFIG_BACKUP="${DIOXUS_CONFIG}.bak"
    cp "$DIOXUS_CONFIG" "$DIOXUS_CONFIG_BACKUP"

    python3 - "$DIOXUS_CONFIG" <<'PY'
from pathlib import Path
import json
import os
import sys

path = Path(sys.argv[1])
jks_file = os.environ["ANDROID_KEYSTORE_FILE"]
jks_password = os.environ["ANDROID_KEYSTORE_PASSWORD"]
key_alias = os.environ["ANDROID_KEY_ALIAS"]
key_password = os.environ["ANDROID_KEY_PASSWORD"]

with path.open("a", encoding="utf-8") as f:
    f.write("\n[bundle.android]\n")
    f.write(f"jks_file = {json.dumps(jks_file)}\n")
    f.write(f"jks_password = {json.dumps(jks_password)}\n")
    f.write(f"key_alias = {json.dumps(key_alias)}\n")
    f.write(f"key_password = {json.dumps(key_password)}\n")
PY
}

configure_outputs() {
    case "$ANDROID_PACKAGE_FORMAT" in
        apk)
            case "$ANDROID_GRADLE_VARIANT" in
                debug)
                    FINAL_GRADLE_TASK="assembleDebug"
                    ARTIFACT_SRC_REL="app/build/outputs/apk/debug/app-debug.apk"
                    ARTIFACT_DST="$PROJECT_ROOT/nostrblue-debug.apk"
                    ARTIFACT_LABEL="APK"
                    ;;
                release)
                    FINAL_GRADLE_TASK="assembleRelease"
                    ARTIFACT_SRC_REL="app/build/outputs/apk/release/app-release.apk"
                    ARTIFACT_DST="$PROJECT_ROOT/nostrblue-release.apk"
                    ARTIFACT_LABEL="APK"
                    ;;
                *)
                    echo "ERROR: Unsupported ANDROID_GRADLE_VARIANT: $ANDROID_GRADLE_VARIANT (expected debug or release)" >&2
                    exit 1
                    ;;
            esac
            ;;
        aab)
            if [ "$ANDROID_GRADLE_VARIANT" != "release" ]; then
                echo "ERROR: ANDROID_GRADLE_VARIANT must be release for Android App Bundles" >&2
                exit 1
            fi
            FINAL_GRADLE_TASK="bundleRelease"
            ARTIFACT_SRC_REL="app/build/outputs/bundle/release/app-release.aab"
            ARTIFACT_DST="$PROJECT_ROOT/nostrblue-release.aab"
            ARTIFACT_LABEL="AAB"
            ;;
        *)
            echo "ERROR: Unsupported ANDROID_PACKAGE_FORMAT: $ANDROID_PACKAGE_FORMAT (expected apk or aab)" >&2
            exit 1
            ;;
    esac
}

build_dx_android() {
    local dx_args=(
        build
        --platform android
        --target aarch64-linux-android
        --no-default-features
        --features mobile
    )

    if [ "$DX_BUILD_PROFILE" = "release" ]; then
        dx_args+=(--release)
    fi

    echo ""
    echo "--- Step 2: dx build (ARM64) ---"
    cd "$PROJECT_ROOT"
    dx "${dx_args[@]}"
    write_android_local_properties
}

# Android SDK/NDK paths
if [ -z "${ANDROID_SDK_ROOT:-}" ]; then
    ANDROID_SDK_ROOT="${ANDROID_HOME:-${HOME}/Android/Sdk}"
fi
if [ -z "${ANDROID_HOME:-}" ]; then
    ANDROID_HOME="$ANDROID_SDK_ROOT"
fi
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    if [ -d "$ANDROID_SDK_ROOT/ndk" ]; then
        NDK_VERSION=$(
            find "$ANDROID_SDK_ROOT/ndk" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; 2>/dev/null \
                | (
                    sort -V 2>/dev/null || python3 -c '
import sys
versions = [line.strip() for line in sys.stdin if line.strip()]
def key(v):
    return tuple(int(part) if part.isdigit() else part for part in v.replace("-", ".").split("."))
if versions:
    print(max(versions, key=key))
'
                ) | tail -n1
        )
        if [ -n "$NDK_VERSION" ]; then
            ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/$NDK_VERSION"
        else
            ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/$NDK_FALLBACK_VERSION"
        fi
    else
        ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/$NDK_FALLBACK_VERSION"
    fi
fi
if [ ! -d "$ANDROID_NDK_HOME" ]; then
    echo "ERROR: ANDROID_NDK_HOME does not exist: $ANDROID_NDK_HOME" >&2
    echo "  Install NDK via: sdkmanager --install 'ndk;$NDK_FALLBACK_VERSION'" >&2
    exit 1
fi
export ANDROID_HOME ANDROID_NDK_HOME ANDROID_SDK_ROOT

# Project paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ANDROID_BUILD_MODE="${ANDROID_BUILD_MODE:-debug}"
ANDROID_GRADLE_VARIANT="${ANDROID_GRADLE_VARIANT:-$ANDROID_BUILD_MODE}"
ANDROID_PACKAGE_FORMAT="${ANDROID_PACKAGE_FORMAT:-apk}"
ANDROID_RUST_PROFILE="${ANDROID_RUST_PROFILE:-debug}"
case "$ANDROID_RUST_PROFILE" in
    debug|release)
        DX_BUILD_PROFILE="$ANDROID_RUST_PROFILE"
        ;;
    *)
        echo "WARNING: Unsupported ANDROID_RUST_PROFILE '$ANDROID_RUST_PROFILE'; defaulting DX build profile to debug" >&2
        DX_BUILD_PROFILE="debug"
        ;;
esac
case "${DX_RELEASE:-}" in
    1|true)
        DX_BUILD_PROFILE="release"
        ;;
esac
DX_ANDROID="$PROJECT_ROOT/target/dx/nostrblue/$DX_BUILD_PROFILE/android/app"
ANDROID_RES_SRC="$PROJECT_ROOT/android/res"
ANDROID_KOTLIN_SRC="$PROJECT_ROOT/android/kotlin"
DIOXUS_CONFIG="$PROJECT_ROOT/Dioxus.toml"
APP_ID="com.nostr.blue"
CARGO_VERSION="$(version_field version)"
ANDROID_VERSION_CODE="$(version_code_from_semver "$CARGO_VERSION")"
GRADLE_APP="$DX_ANDROID/app/build.gradle.kts"
GENERATED_MAIN_ACTIVITY="$DX_ANDROID/app/src/main/kotlin/dev/dioxus/main/MainActivity.kt"
GRADLE_USER_HOME="${GRADLE_USER_HOME:-$PROJECT_ROOT/.gradle-home}"

configure_outputs

trap cleanup EXIT

mkdir -p "$GRADLE_USER_HOME"
export GRADLE_USER_HOME

require_files \
    "$ANDROID_RES_SRC/mipmap-anydpi-v26/ic_launcher.xml" "adaptive launcher XML" \
    "$ANDROID_RES_SRC/mipmap-xxxhdpi/ic_launcher.webp" "launcher icon asset" \
    "$ANDROID_RES_SRC/mipmap-hdpi/ic_launcher_foreground.png" "launcher foreground asset (hdpi)" \
    "$ANDROID_RES_SRC/mipmap-mdpi/ic_launcher_foreground.png" "launcher foreground asset (mdpi)" \
    "$ANDROID_RES_SRC/mipmap-xhdpi/ic_launcher_foreground.png" "launcher foreground asset (xhdpi)" \
    "$ANDROID_RES_SRC/mipmap-xxhdpi/ic_launcher_foreground.png" "launcher foreground asset (xxhdpi)" \
    "$ANDROID_RES_SRC/mipmap-xxxhdpi/ic_launcher_foreground.png" "launcher foreground asset (xxxhdpi)" \
    "$ANDROID_RES_SRC/drawable/ic_launcher_background.xml" "launcher background asset"
require_file "$ANDROID_KOTLIN_SRC/dev/dioxus/main/MediaPlaybackService.kt" "Native playback service source not found"
require_file "$ANDROID_KOTLIN_SRC/dev/dioxus/main/NativeAudioBridge.kt" "Native audio bridge source not found"

echo "=== nostr.blue Android Build ==="
echo "Project: $PROJECT_ROOT"
echo "Package format: $ANDROID_PACKAGE_FORMAT"
echo "NDK: $ANDROID_NDK_HOME"
echo "Version: $CARGO_VERSION ($ANDROID_VERSION_CODE)"
echo "Gradle variant: $ANDROID_GRADLE_VARIANT"
echo "Rust profile: $DX_BUILD_PROFILE"
echo "Gradle home: $GRADLE_USER_HOME"
echo "Android resources: $ANDROID_RES_SRC"

if [ "$ANDROID_GRADLE_VARIANT" = "release" ]; then
    configure_release_signing
fi

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
    -o -path '*/mipmap-anydpi-v26/ic_launcher.xml' \
    -o -path '*/mipmap-anydpi-v26/ic_launcher_round.xml' \) \
    -delete 2>/dev/null || true

echo ""
echo "--- Step 1a: Pre-copy Android resources ---"
mkdir -p "$DX_ANDROID/app/src/main/res/xml"
if cp "$PROJECT_ROOT/android/res/xml/file_paths.xml" "$DX_ANDROID/app/src/main/res/xml/"; then
    echo "Pre-copied file_paths.xml"
else
    echo "ERROR: Failed to pre-copy file_paths.xml into $DX_ANDROID/app/src/main/res/xml/" >&2
    exit 1
fi
if sync_overlay_dir \
    "$ANDROID_KOTLIN_SRC/dev/dioxus/main" \
    "$DX_ANDROID/app/src/main/kotlin/dev/dioxus/main" \
    "MainActivity.kt"
then
    echo "Pre-copied Android Kotlin sources"
else
    echo "ERROR: Failed to pre-copy Android Kotlin sources into $DX_ANDROID/app/src/main/kotlin/dev/dioxus/main/" >&2
    exit 1
fi
write_android_local_properties

build_dx_android

echo ""
echo "--- Step 2b: Clean non-Android files ---"
removed_claude_count=$(find "$DX_ANDROID" -name "CLAUDE.md" -type f -print -delete 2>/dev/null | wc -l | tr -d '[:space:]')
if [ "$removed_claude_count" -gt 0 ]; then
    echo "Cleaned CLAUDE.md files"
else
    echo "No CLAUDE.md files to clean"
fi

echo ""
echo "--- Step 2b.i: Normalize Android metadata ---"
require_file "$GRADLE_APP" "Generated Android Gradle config not found"
require_file "$GENERATED_MAIN_ACTIVITY" "Generated Android MainActivity not found"
normalize_gradle_properties
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
    (r'namespace\s*=\s*"[^"]*"', f'namespace="{app_id}"'),
    (r'applicationId = "[^"]*"', f'applicationId = "{app_id}"'),
    (r'versionName = "[^"]*"', f'versionName = "{version_name}"'),
    (r'versionCode = \d+', f'versionCode = {version_code}'),
]
for pattern, replacement in replacements:
    content, count = re.subn(pattern, replacement, content, count=1)
    if count != 1:
        raise SystemExit(f"failed to patch {pattern} in {path}")

kotlin_options_match = re.search(r'(?ms)^\s*kotlinOptions\s*\{\s*(.*?)^\s*\}\s*', content)
if kotlin_options_match:
    kotlin_options_block = kotlin_options_match.group(0)
    updated_block, count = re.subn(
        r'(?m)^(\s*jvmTarget\s*=\s*)"[^"]+"(\s*)$',
        r'\1"17"\2',
        kotlin_options_block,
        count=1,
    )
    if count != 1:
        raise SystemExit(f"unexpected kotlinOptions format in {path}")
    content = (
        content[:kotlin_options_match.start()]
        + updated_block
        + content[kotlin_options_match.end():]
    )

plugins_block = 'plugins {\n    id("com.android.application")\n    id("org.jetbrains.kotlin.android")\n}\n'
compiler_options_block = '\nkotlin {\n    compilerOptions {\n        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17\n    }\n}\n'
if kotlin_options_match:
    if re.search(r'(?m)^\s*kotlinOptions\s*\{', content) and not re.search(
        r'(?m)^\s*jvmTarget\s*=\s*"17"\s*$',
        content,
    ):
        raise SystemExit(f"unresolved kotlinOptions remains in {path}")
elif "compilerOptions" not in content:
    if plugins_block not in content:
        raise SystemExit(f"failed to find plugins block in {path}")
    content = content.replace(plugins_block, plugins_block + compiler_options_block, 1)

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
verify_gradle_value "namespace" "$APP_ID" '^[[:space:]]*namespace[[:space:]]*=[[:space:]]*"\([^\"]*\)"$'
verify_gradle_value "versionName" "$CARGO_VERSION" '^[[:space:]]*versionName = "\([^\"]*\)"$'
verify_gradle_value "versionCode" "$ANDROID_VERSION_CODE" '^[[:space:]]*versionCode = \([0-9][0-9]*\)$'
echo "Normalized Gradle metadata for $APP_ID"

echo ""
echo "--- Step 2c: Ensure OpenSSL libs ---"
if [ -n "${DX_HOME:-}" ]; then
    OPENSSL_SEARCH="${DX_HOME}/prebuilt"
elif [ -n "${XDG_DATA_HOME:-}" ]; then
    OPENSSL_SEARCH="${XDG_DATA_HOME}/.dx/prebuilt"
elif [ "$(uname -s)" = "Darwin" ]; then
    OPENSSL_SEARCH="$HOME/.dx/prebuilt"
else
    OPENSSL_SEARCH="$HOME/.local/share/.dx/prebuilt"
fi
OPENSSL_PREBUILT=""
if [ -d "$OPENSSL_SEARCH" ]; then
    matches=()
    for dir in "$OPENSSL_SEARCH"/openssl*/ssl/libs/android.arm64-v8a; do
        if [ -f "$dir/libssl.so" ] && [ -f "$dir/libcrypto.so" ]; then
            matches+=("$dir")
        fi
    done
    if [ ${#matches[@]} -gt 0 ]; then
        sorted=()
        while IFS= read -r line; do
            sorted+=("$line")
        done < <(for m in "${matches[@]}"; do
            if mtime=$(stat -c %Y "$m" 2>/dev/null); then
                :
            elif mtime=$(stat -f %m "$m" 2>/dev/null); then
                :
            else
                mtime=0
            fi
            printf '%s\t%s\n' "$mtime" "$m"
        done | sort -rnk1,1 | cut -f2-)
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

echo ""
echo "--- Step 4: Overlay Android resources ---"
if [ -d "$ANDROID_RES_SRC" ]; then
    sync_overlay_dir \
        "$ANDROID_RES_SRC" \
        "$DX_ANDROID/app/src/main/res" \
        "values/strings.xml"
    echo "Copied repo-owned Android resources from android/res"
else
    echo "WARNING: $ANDROID_RES_SRC not found, skipping Android resource overlay"
fi

echo ""
echo "--- Step 4c: Copy Android Kotlin sources ---"
sync_overlay_dir \
    "$ANDROID_KOTLIN_SRC/dev/dioxus/main" \
    "$DX_ANDROID/app/src/main/kotlin/dev/dioxus/main" \
    "MainActivity.kt"
echo "Copied native Android Kotlin sources"

echo ""
echo "--- Step 4d: Verify Android resource overrides ---"
require_files \
    "$DX_ANDROID/app/src/main/res/values/strings.xml" "generated app strings.xml" \
    "$DX_ANDROID/app/src/main/res/xml/file_paths.xml" "generated file_paths.xml" \
    "$DX_ANDROID/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml" "generated adaptive launcher icon" \
    "$DX_ANDROID/app/src/main/res/mipmap-xxxhdpi/ic_launcher.webp" "generated launcher icon density asset" \
    "$DX_ANDROID/app/src/main/res/mipmap-hdpi/ic_launcher_foreground.png" "generated launcher foreground asset (hdpi)" \
    "$DX_ANDROID/app/src/main/res/mipmap-mdpi/ic_launcher_foreground.png" "generated launcher foreground asset (mdpi)" \
    "$DX_ANDROID/app/src/main/res/mipmap-xhdpi/ic_launcher_foreground.png" "generated launcher foreground asset (xhdpi)" \
    "$DX_ANDROID/app/src/main/res/mipmap-xxhdpi/ic_launcher_foreground.png" "generated launcher foreground asset (xxhdpi)" \
    "$DX_ANDROID/app/src/main/res/mipmap-xxxhdpi/ic_launcher_foreground.png" "generated launcher foreground asset (xxxhdpi)"

echo ""
echo "--- Step 5: Run Gradle packaging ---"
cd "$DX_ANDROID"
GRADLE_WRAPPER="$DX_ANDROID/gradlew"
if [ ! -f "$GRADLE_WRAPPER" ] || [ ! -x "$GRADLE_WRAPPER" ]; then
    echo "ERROR: Gradle wrapper missing or not executable at $GRADLE_WRAPPER; cannot run task $FINAL_GRADLE_TASK" >&2
    exit 1
fi
"$GRADLE_WRAPPER" "$FINAL_GRADLE_TASK"

echo ""
echo "--- Step 6: Copy $ARTIFACT_LABEL ---"
ARTIFACT_SRC="$DX_ANDROID/$ARTIFACT_SRC_REL"
if [ -f "$ARTIFACT_SRC" ]; then
    cp "$ARTIFACT_SRC" "$ARTIFACT_DST"
    echo "$ARTIFACT_LABEL: $ARTIFACT_DST"
    if command -v unzip &>/dev/null; then
        echo ""
        if [ "$ANDROID_PACKAGE_FORMAT" = "apk" ]; then
            echo "Native libs in APK:"
            unzip -l "$ARTIFACT_DST" "lib/*" 2>/dev/null | grep "\.so" | awk '{print $NF}' || true
        else
            echo "Bundle contents:"
            unzip -l "$ARTIFACT_DST" | sed -n '1,20p'
        fi
    fi
else
    echo "ERROR: $ARTIFACT_LABEL not found at $ARTIFACT_SRC"
    exit 1
fi

echo ""
echo "=== Build complete ==="
