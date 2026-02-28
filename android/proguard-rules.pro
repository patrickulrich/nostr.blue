# nostr.blue ProGuard rules
# Keep all JNI bridge members in MainActivity (static + instance)
# Static methods are called from Rust via jni::call_static_method()
# Instance fields/methods are needed for ActivityResultLauncher and Intent flow
-keep class dev.dioxus.main.MainActivity { *; }
