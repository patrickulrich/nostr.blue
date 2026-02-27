//! NIP-55 Android Signer
//!
//! Implements `NostrSigner` trait by communicating with external Android
//! signer applications (e.g. Amber) via JNI calls to ContentResolver.
//!
//! Protocol reference: NIP-55 (Android Signer Application)

use std::fmt;
use std::pin::Pin;

use nostr::signer::{NostrSigner, SignerBackend, SignerError};
use nostr::{Event, PublicKey, UnsignedEvent};

/// NIP-55 Android signer that delegates cryptographic operations
/// to an external signer app via Android's ContentResolver.
pub struct Nip55Signer {
    /// The user's public key (obtained during initial connection)
    public_key: PublicKey,
    /// Package name of the signer app (e.g. "com.greenart7c3.nostrsigner")
    signer_package: String,
}

impl fmt::Debug for Nip55Signer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Nip55Signer")
            .field("public_key", &self.public_key)
            .field("signer_package", &self.signer_package)
            .finish()
    }
}

impl Nip55Signer {
    /// Default package name for Amber signer.
    pub fn default_package() -> &'static str {
        "com.greenart7c3.nostrsigner"
    }

    /// Create a new NIP-55 signer with a known public key and signer package.
    ///
    /// The public key and package name should be obtained from the initial
    /// `get_public_key` intent flow when the user first connects their signer.
    pub fn new(public_key: PublicKey, signer_package: String) -> Self {
        Self {
            public_key,
            signer_package,
        }
    }

    /// Check if a NIP-55 signer application is installed on the device.
    pub fn is_signer_installed() -> bool {
        call_static_bool("isSignerInstalled").unwrap_or(false)
    }

    /// Get the signer package name.
    pub fn signer_package(&self) -> &str {
        &self.signer_package
    }
}

impl NostrSigner for Nip55Signer {
    fn backend(&self) -> SignerBackend<'_> {
        SignerBackend::Custom("nip55".into())
    }

    fn get_public_key(&self) -> Pin<Box<dyn std::future::Future<Output = Result<PublicKey, SignerError>> + Send + '_>> {
        let pk = self.public_key;
        Box::pin(async move { Ok(pk) })
    }

    fn sign_event(&self, unsigned: UnsignedEvent) -> Pin<Box<dyn std::future::Future<Output = Result<Event, SignerError>> + Send + '_>> {
        let package = self.signer_package.clone();
        let current_user = self.public_key.to_hex();

        Box::pin(async move {
            let event_json = serde_json::to_string(&unsigned)
                .map_err(SignerError::backend)?;

            let signed_json = call_content_resolver(
                "signEventViaContentResolver",
                &[&event_json, &current_user],
                &package,
            )
            .ok_or_else(|| SignerError::from("NIP-55 signer rejected sign_event or not pre-approved"))?;

            let event: Event = serde_json::from_str(&signed_json)
                .map_err(SignerError::backend)?;

            Ok(event)
        })
    }

    fn nip04_encrypt<'a>(
        &'a self,
        public_key: &'a PublicKey,
        content: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, SignerError>> + Send + 'a>> {
        let package = self.signer_package.clone();
        let current_user = self.public_key.to_hex();
        let pubkey_hex = public_key.to_hex();

        Box::pin(async move {
            call_content_resolver(
                "nip04EncryptViaContentResolver",
                &[content, &pubkey_hex, &current_user],
                &package,
            )
            .ok_or_else(|| SignerError::from("NIP-55 signer rejected nip04_encrypt or not pre-approved"))
        })
    }

    fn nip04_decrypt<'a>(
        &'a self,
        public_key: &'a PublicKey,
        encrypted_content: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, SignerError>> + Send + 'a>> {
        let package = self.signer_package.clone();
        let current_user = self.public_key.to_hex();
        let pubkey_hex = public_key.to_hex();

        Box::pin(async move {
            call_content_resolver(
                "nip04DecryptViaContentResolver",
                &[encrypted_content, &pubkey_hex, &current_user],
                &package,
            )
            .ok_or_else(|| SignerError::from("NIP-55 signer rejected nip04_decrypt or not pre-approved"))
        })
    }

    fn nip44_encrypt<'a>(
        &'a self,
        public_key: &'a PublicKey,
        content: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, SignerError>> + Send + 'a>> {
        let package = self.signer_package.clone();
        let current_user = self.public_key.to_hex();
        let pubkey_hex = public_key.to_hex();

        Box::pin(async move {
            call_content_resolver(
                "nip44EncryptViaContentResolver",
                &[content, &pubkey_hex, &current_user],
                &package,
            )
            .ok_or_else(|| SignerError::from("NIP-55 signer rejected nip44_encrypt or not pre-approved"))
        })
    }

    fn nip44_decrypt<'a>(
        &'a self,
        public_key: &'a PublicKey,
        payload: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, SignerError>> + Send + 'a>> {
        let package = self.signer_package.clone();
        let current_user = self.public_key.to_hex();
        let pubkey_hex = public_key.to_hex();

        Box::pin(async move {
            call_content_resolver(
                "nip44DecryptViaContentResolver",
                &[payload, &pubkey_hex, &current_user],
                &package,
            )
            .ok_or_else(|| SignerError::from("NIP-55 signer rejected nip44_decrypt or not pre-approved"))
        })
    }
}

// ---------------------------------------------------------------------------
// JNI bridge helpers
// ---------------------------------------------------------------------------

/// Find an app class using the app classloader (works from native threads).
///
/// `env.find_class()` uses the system classloader on native threads, which
/// can't see app classes. Instead, we get the app's ClassLoader from the
/// Android context and use `loadClass()`.
fn find_app_class<'a>(
    env: &mut jni::JNIEnv<'a>,
    context: &jni::objects::JObject<'a>,
    class_name: &str,
) -> Option<jni::objects::JClass<'a>> {
    use jni::objects::JValue;

    let context_class = env.get_object_class(context).ok()?;
    let class_loader = env
        .call_method(&context_class, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .ok()?
        .l()
        .ok()?;

    let j_name = env.new_string(class_name).ok()?;
    let loaded = env
        .call_method(
            &class_loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&j_name)],
        )
        .ok()?
        .l()
        .ok()?;

    Some(loaded.into())
}

/// Call a static boolean method on MainActivity companion object.
fn call_static_bool(method_name: &str) -> Option<bool> {
    use jni::objects::JValue;

    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")?;

    let result = env
        .call_static_method(
            class,
            method_name,
            "(Landroid/content/Context;)Z",
            &[JValue::Object(&context)],
        )
        .ok()?
        .z()
        .ok()?;

    Some(result)
}

/// Call a ContentResolver-based static method on MainActivity companion object.
///
/// For sign_event: args = [eventJson, currentUser]
/// For encrypt/decrypt: args = [content, pubkey, currentUser]
fn call_content_resolver(
    method_name: &str,
    args: &[&str],
    signer_package: &str,
) -> Option<String> {
    use jni::objects::{JObject, JValue};

    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let context = unsafe { JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")?;

    let j_signer_package = env.new_string(signer_package).ok()?;

    // Build JNI signature and arguments based on method
    match args.len() {
        // sign_event: (Context, String signerPackage, String eventJson, String currentUser) -> String?
        2 => {
            let j_arg0 = env.new_string(args[0]).ok()?;
            let j_arg1 = env.new_string(args[1]).ok()?;

            let result = env
                .call_static_method(
                    class,
                    method_name,
                    "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                    &[
                        JValue::Object(&context),
                        JValue::Object(&j_signer_package),
                        JValue::Object(&j_arg0),
                        JValue::Object(&j_arg1),
                    ],
                )
                .ok()?
                .l()
                .ok()?;

            if result.is_null() {
                return None;
            }

            let jstr = env.get_string((&result).into()).ok()?;
            Some(jstr.to_string_lossy().into_owned())
        }
        // encrypt/decrypt: (Context, String signerPackage, String content, String pubkey, String currentUser) -> String?
        3 => {
            let j_arg0 = env.new_string(args[0]).ok()?;
            let j_arg1 = env.new_string(args[1]).ok()?;
            let j_arg2 = env.new_string(args[2]).ok()?;

            let result = env
                .call_static_method(
                    class,
                    method_name,
                    "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                    &[
                        JValue::Object(&context),
                        JValue::Object(&j_signer_package),
                        JValue::Object(&j_arg0),
                        JValue::Object(&j_arg1),
                        JValue::Object(&j_arg2),
                    ],
                )
                .ok()?
                .l()
                .ok()?;

            if result.is_null() {
                return None;
            }

            let jstr = env.get_string((&result).into()).ok()?;
            Some(jstr.to_string_lossy().into_owned())
        }
        _ => None,
    }
}
