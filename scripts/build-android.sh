#!/bin/bash
set -e

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
ICON_SOURCE="$PROJECT_ROOT/public/icons/icon-512.png"

if [ ! -f "$ICON_SOURCE" ]; then
    echo "ERROR: Icon source not found: $ICON_SOURCE" >&2
    echo "  Please ensure the icon file exists before building" >&2
    exit 1
fi

echo "=== nostr.blue Android Build ==="
echo "Project: $PROJECT_ROOT"
echo "NDK: $ANDROID_NDK_HOME"

# 1. Clean stale architecture artifacts
echo ""
echo "--- Step 1: Clean stale jniLibs ---"
if [ -d "$DX_ANDROID/app/src/main/jniLibs" ]; then
    rm -rf "$DX_ANDROID/app/src/main/jniLibs"
    echo "Cleaned stale jniLibs"
else
    echo "No stale jniLibs to clean"
fi

# 1a. Pre-copy Android resources (before dx build runs Gradle)
echo ""
echo "--- Step 1a: Pre-copy Android resources ---"
mkdir -p "$PROJECT_ROOT/target/dx/nostrblue/release/android/app/app/src/main/res/xml"
cp "$PROJECT_ROOT/android/res/xml/file_paths.xml" "$PROJECT_ROOT/target/dx/nostrblue/release/android/app/app/src/main/res/xml/" 2>/dev/null && echo "Pre-copied file_paths.xml" || echo "Directory not yet created (will be handled post-build)"

# 2. Build (generates Android project + compiles Rust + runs Gradle)
echo ""
echo "--- Step 2: dx build (ARM64) ---"
cd "$PROJECT_ROOT"
dx build --platform android --release --target aarch64-linux-android --no-default-features --features mobile

# 2b. Clean non-Android files from generated project
echo ""
echo "--- Step 2b: Clean non-Android files ---"
find "$DX_ANDROID" -name "CLAUDE.md" -type f -delete 2>/dev/null && echo "Cleaned CLAUDE.md files" || echo "No CLAUDE.md files to clean"

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

# 4. Fix app name
echo ""
echo "--- Step 4: Fix app name -> nostr.blue ---"
STRINGS_SRC="$PROJECT_ROOT/android/res/values/strings.xml"
STRINGS_DST="$DX_ANDROID/app/src/main/res/values/strings.xml"
if [ -f "$STRINGS_SRC" ]; then
    cp "$STRINGS_SRC" "$STRINGS_DST"
    echo "Copied strings.xml (app_name = nostr.blue)"
else
    echo "WARNING: $STRINGS_SRC not found, skipping app name fix"
fi

# 4b. Copy file_paths.xml for FileProvider
echo ""
echo "--- Step 4b: Copy file_paths.xml ---"
FILE_PATHS_SRC="$PROJECT_ROOT/android/res/xml/file_paths.xml"
FILE_PATHS_DST="$DX_ANDROID/app/src/main/res/xml/file_paths.xml"
if [ -f "$FILE_PATHS_SRC" ]; then
    cp "$FILE_PATHS_SRC" "$FILE_PATHS_DST"
    echo "Copied file_paths.xml (FileProvider paths)"
else
    echo "WARNING: $FILE_PATHS_SRC not found, skipping file_paths.xml"
fi

# 5. Fix app icon
echo ""
echo "--- Step 5: Generate app icons ---"
MIPMAP_BASE="$DX_ANDROID/app/src/main/res"

generate_icons() {
    local tool="$1"

    get_density_size() {
        case "$1" in
            mdpi) echo 48 ;;
            hdpi) echo 72 ;;
            xhdpi) echo 96 ;;
            xxhdpi) echo 144 ;;
            xxxhdpi) echo 192 ;;
            *) echo ""; return 1 ;;
        esac
    }

    # Temp dir only needed for cwebp tool
    local tmp_dir=""
    if [ "$tool" = "cwebp" ]; then
        tmp_dir=$(mktemp -d "/tmp/ic_launcher_XXXXXX")
        trap "rm -rf \"$tmp_dir\"" EXIT
    fi

    for density in mdpi hdpi xhdpi xxhdpi xxxhdpi; do
        local size
        size=$(get_density_size "$density") || { echo "ERROR: Unknown density: $density"; exit 1; }
        local dir="$MIPMAP_BASE/mipmap-${density}"
        mkdir -p "$dir"

        if [ "$tool" = "convert" ]; then
            # ImageMagick: resize PNG then convert to WEBP
            convert "$ICON_SOURCE" -resize "${size}x${size}" "$dir/ic_launcher.webp"
            convert "$ICON_SOURCE" -resize "${size}x${size}" "$dir/ic_launcher_round.webp"
            # Foreground layer (with padding for adaptive icon safe zone)
            local fg_size=$((size * 108 / 72))
            convert "$ICON_SOURCE" -resize "${size}x${size}" \
                -gravity center -background none -extent "${fg_size}x${fg_size}" \
                "$dir/ic_launcher_foreground.webp" 2>/dev/null
        elif [ "$tool" = "cwebp" ]; then
            # cwebp: need intermediate PNG via ffmpeg
            local tmp_png="$tmp_dir/ic_launcher_${density}.png"
            local fg_size=$((size * 108 / 72))
            ffmpeg -y -i "$ICON_SOURCE" -vf "scale=${size}:${size}" "$tmp_png" 2>/dev/null
            cwebp -q 90 "$tmp_png" -o "$dir/ic_launcher.webp"
            cp "$dir/ic_launcher.webp" "$dir/ic_launcher_round.webp"
            ffmpeg -y -i "$ICON_SOURCE" -vf "scale=${size}:${size},pad=${fg_size}:${fg_size}:(ow-iw)/2:(oh-ih)/2:color=0x00000000" \
                "$dir/ic_launcher_foreground.webp" 2>/dev/null
        elif [ "$tool" = "ffmpeg" ]; then
            # ffmpeg can output webp directly
            ffmpeg -y -i "$ICON_SOURCE" -vf "scale=${size}:${size}" "$dir/ic_launcher.webp" 2>/dev/null
            cp "$dir/ic_launcher.webp" "$dir/ic_launcher_round.webp"
            # Foreground
            local fg_size=$((size * 108 / 72))
            ffmpeg -y -i "$ICON_SOURCE" \
                -vf "scale=${size}:${size},pad=${fg_size}:${fg_size}:(ow-iw)/2:(oh-ih)/2:color=0x00000000" \
                "$dir/ic_launcher_foreground.webp" 2>/dev/null
        fi
        echo "  ${density}: ${size}x${size}px"
    done

    # Cleanup temp dir if created
    [ -n "$tmp_dir" ] && rm -rf "$tmp_dir"
}

if command -v convert &>/dev/null; then
    echo "Using ImageMagick"
    generate_icons "convert"
elif command -v cwebp &>/dev/null && command -v ffmpeg &>/dev/null; then
    echo "Using cwebp + ffmpeg"
    generate_icons "cwebp"
elif command -v ffmpeg &>/dev/null; then
    echo "Using ffmpeg"
    generate_icons "ffmpeg"
else
    echo "WARNING: No image tools found (install imagemagick, ffmpeg, or cwebp)"
    echo "  Skipping icon generation. Install with:"
    echo "    sudo apt install imagemagick   # or"
    echo "    sudo apt install ffmpeg"
fi

# 6. Re-run Gradle to pick up icon/name/proguard changes
echo ""
echo "--- Step 6: Re-run Gradle ---"
cd "$DX_ANDROID"
./gradlew assembleDebug

# 7. Copy output APK
echo ""
echo "--- Step 7: Copy APK ---"
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
