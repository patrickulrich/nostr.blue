# Vendored OpenSSL prebuilt (Android arm64-v8a, 16 KB page size)

`libssl.so` / `libcrypto.so` rebuilt from the same OpenSSL 1.1.1q sources the
dx CLI bundles, with 16 KB ELF LOAD-segment alignment for Google Play's
16 KB page-size requirement on apps targeting API 35+ (required since
Nov 2025). The dx CLI's bundled prebuilts are 4 KB-aligned and trigger the
system `PageSizeMismatchDialog` on 16 KB kernel devices (e.g. Pixel 10).

## Build recipe

```bash
export ANDROID_NDK_HOME=$ANDROID_SDK_ROOT/ndk/29.0.14206865
export PATH=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH

curl -LO https://www.openssl.org/source/old/1.1.1/openssl-1.1.1q.tar.gz
tar xzf openssl-1.1.1q.tar.gz && cd openssl-1.1.1q
perl Configure android-arm64 -D__ANDROID_API__=26 shared \
    -Wl,-z,max-page-size=16384 no-tests
make -j"$(nproc)"
cp libssl.so libcrypto.so <this directory>
```

Verify with `readelf -lW libssl.so | awk '/^  LOAD/ {print $NF}'` — must
print `0x4000`.

## License

OpenSSL 1.1.1q is distributed under the OpenSSL License (Apache-style,
permissive, redistribution allowed). See
https://www.openssl.org/source/license.html.
