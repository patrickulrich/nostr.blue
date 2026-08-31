//! Provides a stable api to rust crates for interfacing with the Android platform. It is
//! initialized by the runtime, usually [__ndk-glue__](https://crates.io/ndk-glue),
//! but could also be initialized by Java or Kotlin code when embedding in an existing Android
//! project.
//!
//! Vendored from ndk-context 0.1.1 with nostr.blue modifications (see the
//! `initialize_android_context` / `release_android_context` doc comments).
//! Upstream stores the context in a `static mut` accessed via raw borrows;
//! keep that design (AndroidContext is Copy and written once in practice)
//! but silence the rust-2024 `static_mut_refs` lint it now triggers.
//!
//! ```no_run
//! let ctx = ndk_context::android_context();
//! let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }?;
//! let env = vm.attach_current_thread();
//! let class_ctx = env.find_class("android/content/Context")?;
//! let audio_service = env.get_static_field(class_ctx, "AUDIO_SERVICE", "Ljava/lang/String;")?;
//! let audio_manager = env
//!     .call_method(
//!         ctx.context() as jni::sys::jobject,
//!         "getSystemService",
//!         "(Ljava/lang/String;)Ljava/lang/Object;",
//!         &[audio_service],
//!     )?
//!     .l()?;
//! ```
#![allow(static_mut_refs)]
use std::ffi::c_void;

static mut ANDROID_CONTEXT: Option<AndroidContext> = None;

/// [`AndroidContext`] provides the pointers required to interface with the jni on Android
/// platforms.
#[derive(Clone, Copy, Debug)]
pub struct AndroidContext {
    java_vm: *mut c_void,
    context_jobject: *mut c_void,
}

impl AndroidContext {
    /// A handle to the `JavaVM` object.
    ///
    /// Usage with [__jni__](https://crates.io/crates/jni) crate:
    /// ```no_run
    /// let ctx = ndk_context::android_context();
    /// let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }?;
    /// let env = vm.attach_current_thread();
    /// ```
    pub fn vm(self) -> *mut c_void {
        self.java_vm
    }

    /// A handle to an [android.content.Context](https://developer.android.com/reference/android/content/Context).
    /// In most cases this will be a ptr to an `Activity`, but this isn't guaranteed.
    ///
    /// Usage with [__jni__](https://crates.io/crates/jni) crate:
    /// ```no_run
    /// let ctx = ndk_context::android_context();
    /// let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }?;
    /// let env = vm.attach_current_thread();
    /// let class_ctx = env.find_class("android/content/Context")?;
    /// let audio_service = env.get_static_field(class_ctx, "AUDIO_SERVICE", "Ljava/lang/String;")?;
    /// let audio_manager = env
    ///     .call_method(
    ///         ctx.context() as jni::sys::jobject,
    ///         "getSystemService",
    ///         "(Ljava/lang/String;)Ljava/lang/Object;",
    ///         &[audio_service],
    ///     )?
    ///     .l()?;
    /// ```
    pub fn context(self) -> *mut c_void {
        self.context_jobject
    }
}

/// Main entry point to this crate. Returns an [`AndroidContext`].
pub fn android_context() -> AndroidContext {
    unsafe { ANDROID_CONTEXT.expect("android context was not initialized") }
}

/// Initializes the [`AndroidContext`]. [`AndroidContext`] is initialized by [__ndk-glue__](https://crates.io/crates/ndk-glue)
/// before `main` is called.
///
/// # Safety
///
/// The pointers must be valid and this function must be called at least once before `main` is
/// called.
///
/// nostr.blue modification: idempotent. Android may re-create the Activity
/// (theme change, rotation, memory pressure); the new activity's `onCreate`
/// runs BEFORE the old activity's `onDestroy` releases the context, so
/// upstream's `assert!(previous.is_none())` aborts the process on every
/// re-entry. First initialization wins; later calls are no-ops (the JavaVM
/// pointer is identical for the process lifetime and the context pointer is
/// a GlobalRef held for process lifetime by the windowing layer).
pub unsafe fn initialize_android_context(java_vm: *mut c_void, context_jobject: *mut c_void) {
    let _ = ANDROID_CONTEXT.replace(AndroidContext {
        java_vm,
        context_jobject,
    });
}

/// Removes the [`AndroidContext`]. It is released by [__ndk-glue__](https://crates.io/crates/ndk-glue)
/// when the activity is finished and destroyed.
///
/// # Safety
///
/// This function must only be called after [`initialize_android_context()`],
/// when the activity is subsequently destroyed according to Android.
///
/// nostr.blue modification: no-op. Activity destruction does not imply the
/// process is going away (a replacement activity may already have been
/// created); clearing the global would break every JNI consumer
/// mid-process. The global is intentionally held until process exit.
pub unsafe fn release_android_context() {
    // Intentionally empty — see doc comment above.
}
