# nostr.blue ProGuard rules
# Keep all static JNI bridge methods in MainActivity companion object
# These are called from Rust via jni::call_static_method()
-keep class dev.dioxus.main.MainActivity {
    static <methods>;
}
-keepclassmembers class dev.dioxus.main.MainActivity {
    static <methods>;
}
