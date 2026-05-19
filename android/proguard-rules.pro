# nostr.blue ProGuard rules
# Keep all JNI bridge members in MainActivity (static + instance)
# Static methods are called from Rust via jni::call_static_method()
# Instance fields/methods are needed for ActivityResultLauncher and Intent flow
-keep class dev.dioxus.main.MainActivity { *; }
# Android Auto browse/cache classes (called via reflection by Media3 and SharedPreferences)
-keep class dev.dioxus.main.BrowseCache { *; }
-keep class dev.dioxus.main.MediaBrowseTree { *; }
-keep class dev.dioxus.main.WavlakeClient { *; }
-keep class dev.dioxus.main.NativeAudioBridge { *; }
-keep class dev.dioxus.main.MediaPlaybackService { *; }
# Media3 (ExoPlayer) - rules bundled in AARs but explicit for safety
-keep class androidx.media3.** { *; }
-dontwarn androidx.media3.**
