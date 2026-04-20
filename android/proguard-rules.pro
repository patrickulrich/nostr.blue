# nostr.blue ProGuard rules
# Keep all JNI bridge members in MainActivity (static + instance)
# Static methods are called from Rust via jni::call_static_method()
# Instance fields/methods are needed for ActivityResultLauncher and Intent flow
-keep class dev.dioxus.main.MainActivity { *; }
# Audio plugin (manganis::ffi managed)
-keep class com.nostr.blue.audio.** { *; }
# Media3 (ExoPlayer) - rules bundled in AARs but explicit for safety
-keep class androidx.media3.** { *; }
-dontwarn androidx.media3.**
