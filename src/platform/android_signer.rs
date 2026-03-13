//! NIP-55 Android Signer
//!
//! Implements `NostrSigner` trait by communicating with external Android
//! signer applications (e.g. Amber) via JNI calls to ContentResolver
//! and Intent-based approval flows.
//!
//! Protocol reference: NIP-55 (Android Signer Application)

use std::fmt;
use std::pin::Pin;
use std::time::{Duration, Instant};

use nostr::secp256k1::schnorr::Signature;
use nostr::signer::{NostrSigner, SignerBackend, SignerError};
use nostr::{Event, PublicKey, UnsignedEvent};

const SIGN_EVENT_REJECTED: &str = "NIP-55 signer rejected sign_event or not pre-approved";
const APPROVAL_POLL_INTERVAL: Duration = Duration::from_millis(500);
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(45);

/// Result of polling for a pending Intent result from the signer app.
#[derive(Debug, Clone)]
pub enum IntentPollResult {
    /// The signer returned a public key and package name.
    Ready { pubkey: PublicKey, package: String },
    /// An Intent is currently in flight (user hasn't returned yet).
    InFlight,
    /// No pending result and no Intent in flight.
    None,
    /// The Intent failed or was rejected by the user.
    Error(String),
}

#[derive(Debug, Clone)]
pub enum OperationIntentPollResult {
    Ready {
        result: Option<String>,
        event: Option<String>,
        package: Option<String>,
        rejected: bool,
    },
    InFlight,
    None,
    Error(String),
}

enum ForegroundOperation<'a> {
    SignEvent {
        unsigned: UnsignedEvent,
        event_json: String,
        current_user: &'a str,
    },
    Nip04Encrypt {
        content: &'a str,
        pubkey_hex: &'a str,
        current_user: &'a str,
    },
    Nip04Decrypt {
        content: &'a str,
        pubkey_hex: &'a str,
        current_user: &'a str,
    },
    Nip44Encrypt {
        content: &'a str,
        pubkey_hex: &'a str,
        current_user: &'a str,
    },
    Nip44Decrypt {
        content: &'a str,
        pubkey_hex: &'a str,
        current_user: &'a str,
    },
}

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

    /// Get package names of all installed NIP-55 signer apps.
    ///
    /// Splits on comma — safe because Android package names are restricted to
    /// `[a-zA-Z0-9_.]` per the Android manifest spec.
    pub fn get_signer_packages() -> Vec<String> {
        call_static_string("getSignerPackages")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .collect()
    }

    /// Request the user's public key from a signer app via ContentResolver.
    ///
    /// Only succeeds if the user has previously approved this app in the signer.
    /// Returns `None` for first-time connections (Intent-based approval needed).
    pub fn request_public_key(signer_package: &str) -> Option<PublicKey> {
        let hex = call_content_resolver_simple("getPublicKeyViaContentResolver", signer_package)?;
        match PublicKey::parse(&hex) {
            Ok(pk) => Some(pk),
            Err(e) => {
                log::error!("NIP-55 JNI: failed to parse public key '{}': {}", hex, e);
                None
            }
        }
    }

    /// Launch the get_public_key Intent to open the signer app for user approval.
    ///
    /// Returns `Ok(true)` if the Intent was launched, `Ok(false)` if already in flight.
    /// Returns `Err` on failure.
    pub fn launch_get_public_key() -> Result<bool, String> {
        let result = call_static_string("launchGetPublicKey")
            .unwrap_or_else(|| "error:JNI call failed".to_string());

        match result.as_str() {
            "launched" => {
                log::info!("NIP-55: get_public_key Intent launched");
                Ok(true)
            }
            "already_in_flight" => {
                log::info!("NIP-55: Intent already in flight");
                Ok(false)
            }
            "no_instance" => {
                log::error!("NIP-55: no Activity instance available");
                Err("No Activity instance available".to_string())
            }
            other if other.starts_with("error:") => {
                let msg = other.strip_prefix("error:").unwrap_or(other);
                log::error!("NIP-55: launchGetPublicKey error: {}", msg);
                Err(msg.to_string())
            }
            other => {
                log::error!("NIP-55: unexpected launchGetPublicKey result: {}", other);
                Err(format!("Unexpected result: {}", other))
            }
        }
    }

    /// Launch the get_public_key intent for a specific signer package so an
    /// already-connected signer can refresh permissions in place.
    pub fn launch_get_public_key_for_package(signer_package: &str) -> Result<bool, String> {
        let result = call_static_string_with_arg("launchGetPublicKeyForPackage", signer_package)
            .unwrap_or_else(|| "error:JNI call failed".to_string());

        match result.as_str() {
            "launched" => {
                log::info!(
                    "NIP-55: package-scoped get_public_key Intent launched for {}",
                    signer_package
                );
                Ok(true)
            }
            "already_in_flight" => {
                log::info!("NIP-55: Intent already in flight for {}", signer_package);
                Ok(false)
            }
            "no_instance" => {
                log::error!("NIP-55: no Activity instance available");
                Err("No Activity instance available".to_string())
            }
            other if other.starts_with("error:") => {
                let msg = other.strip_prefix("error:").unwrap_or(other);
                log::error!(
                    "NIP-55: launchGetPublicKeyForPackage error for {}: {}",
                    signer_package,
                    msg
                );
                Err(msg.to_string())
            }
            other => {
                log::error!(
                    "NIP-55: unexpected launchGetPublicKeyForPackage result for {}: {}",
                    signer_package,
                    other
                );
                Err(format!("Unexpected result: {}", other))
            }
        }
    }

    /// Poll for the result of a previously launched get_public_key Intent.
    pub fn poll_intent_result() -> IntentPollResult {
        // Check if Intent is still in flight
        let in_flight = call_static_bool("isIntentInFlight").unwrap_or(false);
        if in_flight {
            return IntentPollResult::InFlight;
        }

        // Check for error
        if let Some(err) = call_static_string("pollIntentError") {
            log::warn!("NIP-55: Intent error: {}", err);
            return IntentPollResult::Error(err);
        }

        // Check for pubkey result
        if let Some(pubkey_hex) = call_static_string("pollPublicKeyResult") {
            let package = match call_static_string("pollPackageResult") {
                Some(p) => p,
                None => {
                    return IntentPollResult::Error(
                        "No package name returned from signer".to_string(),
                    );
                }
            };
            match PublicKey::parse(&pubkey_hex) {
                Ok(pubkey) => IntentPollResult::Ready { pubkey, package },
                Err(e) => {
                    log::error!(
                        "NIP-55: failed to parse Intent pubkey '{}': {}",
                        pubkey_hex,
                        e
                    );
                    IntentPollResult::Error(format!("Invalid pubkey from signer: {}", e))
                }
            }
        } else {
            IntentPollResult::None
        }
    }

    /// Clear any pending Intent result state.
    pub fn clear_pending_result() {
        call_static_void("clearPendingResult");
        log::debug!("NIP-55: cleared pending Intent state");
    }

    pub fn poll_operation_result() -> OperationIntentPollResult {
        let in_flight = call_static_bool("isIntentInFlight").unwrap_or(false);
        if in_flight {
            return OperationIntentPollResult::InFlight;
        }

        if let Some(err) = call_static_string("pollIntentError") {
            log::warn!("NIP-55: operation Intent error: {}", err);
            return OperationIntentPollResult::Error(err);
        }

        let result = call_static_string("pollOperationResult");
        let event = call_static_string("pollOperationEvent");
        let package = call_static_string("pollOperationPackage");
        let rejected = call_static_string("pollOperationRejected")
            .and_then(|value| value.parse::<bool>().ok());

        if result.is_some() || event.is_some() || package.is_some() || rejected.is_some() {
            return OperationIntentPollResult::Ready {
                result,
                event,
                package,
                rejected: rejected.unwrap_or(false),
            };
        }

        OperationIntentPollResult::None
    }

    /// Get the signer package name.
    pub fn signer_package(&self) -> &str {
        &self.signer_package
    }

    fn validate_signed_event(
        unsigned: &UnsignedEvent,
        signed_json: &str,
        current_user: &str,
    ) -> Result<Event, SignerError> {
        let event: Event = serde_json::from_str(signed_json).map_err(SignerError::backend)?;

        if event.pubkey.to_hex() != current_user {
            return Err(SignerError::from("signer pubkey mismatch"));
        }

        if event.kind != unsigned.kind {
            return Err(SignerError::from("signed event payload mismatch: kind"));
        }
        if event.tags != unsigned.tags {
            return Err(SignerError::from("signed event payload mismatch: tags"));
        }
        if event.content != unsigned.content {
            return Err(SignerError::from("signed event payload mismatch: content"));
        }
        if event.created_at != unsigned.created_at {
            return Err(SignerError::from(
                "signed event payload mismatch: created_at",
            ));
        }

        Ok(event)
    }

    fn validate_signature_result(
        unsigned: UnsignedEvent,
        signature_hex: &str,
    ) -> Result<Event, SignerError> {
        let sig = signature_hex
            .parse::<Signature>()
            .map_err(SignerError::backend)?;
        unsigned.add_signature(sig).map_err(SignerError::backend)
    }

    fn launch_foreground_operation<'a>(
        signer_package: &str,
        operation: &ForegroundOperation<'a>,
    ) -> Result<bool, String> {
        let result = match operation {
            ForegroundOperation::SignEvent {
                event_json,
                current_user,
                ..
            } => call_static_string_with_args(
                "launchSignEventIntent",
                &[signer_package, event_json, current_user],
            ),
            ForegroundOperation::Nip04Encrypt {
                content,
                pubkey_hex,
                current_user,
            } => call_static_string_with_args(
                "launchNip04EncryptIntent",
                &[signer_package, content, pubkey_hex, current_user],
            ),
            ForegroundOperation::Nip04Decrypt {
                content,
                pubkey_hex,
                current_user,
            } => call_static_string_with_args(
                "launchNip04DecryptIntent",
                &[signer_package, content, pubkey_hex, current_user],
            ),
            ForegroundOperation::Nip44Encrypt {
                content,
                pubkey_hex,
                current_user,
            } => call_static_string_with_args(
                "launchNip44EncryptIntent",
                &[signer_package, content, pubkey_hex, current_user],
            ),
            ForegroundOperation::Nip44Decrypt {
                content,
                pubkey_hex,
                current_user,
            } => call_static_string_with_args(
                "launchNip44DecryptIntent",
                &[signer_package, content, pubkey_hex, current_user],
            ),
        }
        .unwrap_or_else(|| "error:JNI call failed".to_string());

        match result.as_str() {
            "launched" => Ok(true),
            "already_in_flight" => Ok(false),
            "no_instance" => Err("No Activity instance available".to_string()),
            other if other.starts_with("error:") => {
                Err(other.strip_prefix("error:").unwrap_or(other).to_string())
            }
            other => Err(format!("Unexpected result: {}", other)),
        }
    }

    async fn wait_for_foreground_operation<'a>(
        &self,
        operation: ForegroundOperation<'a>,
    ) -> Result<String, SignerError> {
        let start = Instant::now();

        loop {
            match Self::poll_operation_result() {
                OperationIntentPollResult::Ready {
                    result,
                    event,
                    package,
                    rejected,
                } => {
                    Self::clear_pending_result();

                    if let Some(package) = package.as_deref() {
                        if package != self.signer_package {
                            return Err(SignerError::from(format!(
                                "NIP-55 signer operation returned unexpected package {}",
                                package
                            )));
                        }
                    }

                    if rejected {
                        return Err(SignerError::from(
                            "NIP-55 signer request was rejected by the user",
                        ));
                    }

                    return match operation {
                        ForegroundOperation::SignEvent {
                            unsigned,
                            current_user,
                            ..
                        } => {
                            if let Some(event_json) = event {
                                let event = Self::validate_signed_event(
                                    &unsigned,
                                    &event_json,
                                    current_user,
                                )?;
                                serde_json::to_string(&event).map_err(SignerError::backend)
                            } else if let Some(signature) = result {
                                let event = Self::validate_signature_result(unsigned, &signature)?;
                                serde_json::to_string(&event).map_err(SignerError::backend)
                            } else {
                                Err(SignerError::from(
                                    "NIP-55 signer returned no sign_event result",
                                ))
                            }
                        }
                        ForegroundOperation::Nip04Encrypt { .. }
                        | ForegroundOperation::Nip04Decrypt { .. }
                        | ForegroundOperation::Nip44Encrypt { .. }
                        | ForegroundOperation::Nip44Decrypt { .. } => result.ok_or_else(|| {
                            SignerError::from(
                                "NIP-55 signer returned no payload for the requested operation",
                            )
                        }),
                    };
                }
                OperationIntentPollResult::InFlight | OperationIntentPollResult::None => {
                    if start.elapsed() >= APPROVAL_TIMEOUT {
                        Self::clear_pending_result();
                        return Err(SignerError::from(
                            "NIP-55 signer operation approval timed out",
                        ));
                    }
                    tokio::time::sleep(APPROVAL_POLL_INTERVAL).await;
                }
                OperationIntentPollResult::Error(err) => {
                    Self::clear_pending_result();
                    return Err(SignerError::from(format!(
                        "NIP-55 signer operation failed: {}",
                        err
                    )));
                }
            }
        }
    }
}

impl NostrSigner for Nip55Signer {
    fn backend(&self) -> SignerBackend<'_> {
        SignerBackend::Custom("nip55".into())
    }

    fn get_public_key(
        &self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PublicKey, SignerError>> + Send + '_>>
    {
        let pk = self.public_key;
        Box::pin(async move { Ok(pk) })
    }

    fn sign_event(
        &self,
        unsigned: UnsignedEvent,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Event, SignerError>> + Send + '_>> {
        let package = self.signer_package.clone();
        let current_user = self.public_key.to_hex();

        Box::pin(async move {
            let event_json = serde_json::to_string(&unsigned).map_err(SignerError::backend)?;
            let kind = unsigned.kind.as_u16();

            if let Some(signed_json) = call_content_resolver(
                "signEventViaContentResolver",
                &[&event_json, &current_user],
                &package,
            ) {
                return Self::validate_signed_event(&unsigned, &signed_json, &current_user);
            }

            log::info!(
                "NIP-55: sign_event for kind {} was not pre-approved; requesting exact foreground approval",
                kind
            );

            let operation = ForegroundOperation::SignEvent {
                unsigned: unsigned.clone(),
                event_json,
                current_user: &current_user,
            };

            Self::launch_foreground_operation(&package, &operation).map_err(|e| {
                SignerError::from(format!("{} (kind {}): {}", SIGN_EVENT_REJECTED, kind, e))
            })?;

            let signed_json = self.wait_for_foreground_operation(operation).await?;

            Self::validate_signed_event(&unsigned, &signed_json, &current_user)
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
            if let Some(ciphertext) = call_content_resolver(
                "nip04EncryptViaContentResolver",
                &[content, &pubkey_hex, &current_user],
                &package,
            ) {
                return Ok(ciphertext);
            }

            let operation = ForegroundOperation::Nip04Encrypt {
                content,
                pubkey_hex: &pubkey_hex,
                current_user: &current_user,
            };

            Self::launch_foreground_operation(&package, &operation).map_err(|e| {
                SignerError::from(format!(
                    "NIP-55 signer rejected nip04_encrypt or not pre-approved: {}",
                    e
                ))
            })?;

            self.wait_for_foreground_operation(operation).await
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
            if let Some(plaintext) = call_content_resolver(
                "nip04DecryptViaContentResolver",
                &[encrypted_content, &pubkey_hex, &current_user],
                &package,
            ) {
                return Ok(plaintext);
            }

            let operation = ForegroundOperation::Nip04Decrypt {
                content: encrypted_content,
                pubkey_hex: &pubkey_hex,
                current_user: &current_user,
            };

            Self::launch_foreground_operation(&package, &operation).map_err(|e| {
                SignerError::from(format!(
                    "NIP-55 signer rejected nip04_decrypt or not pre-approved: {}",
                    e
                ))
            })?;

            self.wait_for_foreground_operation(operation).await
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
            if let Some(ciphertext) = call_content_resolver(
                "nip44EncryptViaContentResolver",
                &[content, &pubkey_hex, &current_user],
                &package,
            ) {
                return Ok(ciphertext);
            }

            let operation = ForegroundOperation::Nip44Encrypt {
                content,
                pubkey_hex: &pubkey_hex,
                current_user: &current_user,
            };

            Self::launch_foreground_operation(&package, &operation).map_err(|e| {
                SignerError::from(format!(
                    "NIP-55 signer rejected nip44_encrypt or not pre-approved: {}",
                    e
                ))
            })?;

            self.wait_for_foreground_operation(operation).await
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
            if let Some(plaintext) = call_content_resolver(
                "nip44DecryptViaContentResolver",
                &[payload, &pubkey_hex, &current_user],
                &package,
            ) {
                return Ok(plaintext);
            }

            let operation = ForegroundOperation::Nip44Decrypt {
                content: payload,
                pubkey_hex: &pubkey_hex,
                current_user: &current_user,
            };

            Self::launch_foreground_operation(&package, &operation).map_err(|e| {
                SignerError::from(format!(
                    "NIP-55 signer rejected nip44_decrypt or not pre-approved: {}",
                    e
                ))
            })?;

            self.wait_for_foreground_operation(operation).await
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

    let context_class = match env.get_object_class(context) {
        Ok(c) => c,
        Err(e) => {
            log::error!("NIP-55 JNI: get_object_class failed: {}", e);
            return None;
        }
    };
    let class_loader = match env.call_method(
        &context_class,
        "getClassLoader",
        "()Ljava/lang/ClassLoader;",
        &[],
    ) {
        Ok(v) => match v.l() {
            Ok(l) => l,
            Err(e) => {
                log::error!("NIP-55 JNI: getClassLoader .l() failed: {}", e);
                return None;
            }
        },
        Err(e) => {
            log::error!("NIP-55 JNI: getClassLoader call failed: {}", e);
            return None;
        }
    };

    let j_name = match env.new_string(class_name) {
        Ok(s) => s,
        Err(e) => {
            log::error!("NIP-55 JNI: new_string('{}') failed: {}", class_name, e);
            return None;
        }
    };
    let loaded = match env.call_method(
        &class_loader,
        "loadClass",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        &[JValue::Object(&j_name)],
    ) {
        Ok(v) => match v.l() {
            Ok(l) => l,
            Err(e) => {
                log::error!(
                    "NIP-55 JNI: loadClass .l() failed for '{}': {}",
                    class_name,
                    e
                );
                return None;
            }
        },
        Err(e) => {
            log::error!("NIP-55 JNI: loadClass('{}') failed: {}", class_name, e);
            return None;
        }
    };

    Some(loaded.into())
}

/// Call a static method on MainActivity that takes Context and returns String.
fn call_static_string(method_name: &str) -> Option<String> {
    use jni::objects::JValue;

    let ctx = ndk_context::android_context();
    let vm = match unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(v) => v,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: JavaVM::from_raw failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };
    let mut env = match vm.attach_current_thread() {
        Ok(e) => e,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: attach_current_thread failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };

    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")?;

    let result = match env.call_static_method(
        class,
        method_name,
        "(Landroid/content/Context;)Ljava/lang/String;",
        &[JValue::Object(&context)],
    ) {
        Ok(v) => match v.l() {
            Ok(l) => l,
            Err(e) => {
                log::error!("NIP-55 JNI: {}.l() failed: {}", method_name, e);
                return None;
            }
        },
        Err(e) => {
            log::error!("NIP-55 JNI: {} call failed: {}", method_name, e);
            return None;
        }
    };

    if result.is_null() {
        return None;
    }

    let ret = match env.get_string((&result).into()) {
        Ok(jstr) => Some(jstr.to_string_lossy().into_owned()),
        Err(e) => {
            log::error!("NIP-55 JNI: get_string failed in {}: {}", method_name, e);
            None
        }
    };
    ret
}

/// Call a static method on MainActivity that takes Context and String and returns String.
fn call_static_string_with_arg(method_name: &str, arg: &str) -> Option<String> {
    use jni::objects::{JObject, JValue};

    let ctx = ndk_context::android_context();
    let vm = match unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(v) => v,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: JavaVM::from_raw failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };
    let mut env = match vm.attach_current_thread() {
        Ok(e) => e,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: attach_current_thread failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };

    let context = unsafe { JObject::from_raw(ctx.context().cast()) };
    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")?;

    let j_arg = match env.new_string(arg) {
        Ok(s) => s,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: new_string('{}') failed in {}: {}",
                arg,
                method_name,
                e
            );
            return None;
        }
    };

    let result = match env.call_static_method(
        class,
        method_name,
        "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
        &[JValue::Object(&context), JValue::Object(&j_arg)],
    ) {
        Ok(v) => match v.l() {
            Ok(l) => l,
            Err(e) => {
                log::error!("NIP-55 JNI: {}.l() failed: {}", method_name, e);
                return None;
            }
        },
        Err(e) => {
            log::error!("NIP-55 JNI: {} call failed: {}", method_name, e);
            return None;
        }
    };

    if result.is_null() {
        return None;
    }

    let ret = match env.get_string((&result).into()) {
        Ok(jstr) => Some(jstr.to_string_lossy().into_owned()),
        Err(e) => {
            log::error!("NIP-55 JNI: get_string failed in {}: {}", method_name, e);
            None
        }
    };
    ret
}

/// Call a static method on MainActivity that takes Context and N strings and returns String.
fn call_static_string_with_args(method_name: &str, args: &[&str]) -> Option<String> {
    use jni::objects::{JObject, JValue};

    let ctx = ndk_context::android_context();
    let vm = match unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(v) => v,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: JavaVM::from_raw failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };
    let mut env = match vm.attach_current_thread() {
        Ok(e) => e,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: attach_current_thread failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };

    let context = unsafe { JObject::from_raw(ctx.context().cast()) };
    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")?;

    match args.len() {
        3 => {
            let j_arg0 = env.new_string(args[0]).ok()?;
            let j_arg1 = env.new_string(args[1]).ok()?;
            let j_arg2 = env.new_string(args[2]).ok()?;

            let result = match env.call_static_method(
                class,
                method_name,
                "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[
                    JValue::Object(&context),
                    JValue::Object(&j_arg0),
                    JValue::Object(&j_arg1),
                    JValue::Object(&j_arg2),
                ],
            ) {
                Ok(v) => match v.l() {
                    Ok(l) => l,
                    Err(e) => {
                        log::error!("NIP-55 JNI: {}.l() failed: {}", method_name, e);
                        return None;
                    }
                },
                Err(e) => {
                    log::error!("NIP-55 JNI: {} call failed: {}", method_name, e);
                    return None;
                }
            };

            if result.is_null() {
                return None;
            }

            let ret = match env.get_string((&result).into()) {
                Ok(jstr) => Some(jstr.to_string_lossy().into_owned()),
                Err(e) => {
                    log::error!("NIP-55 JNI: get_string failed in {}: {}", method_name, e);
                    None
                }
            };
            ret
        }
        4 => {
            let j_arg0 = env.new_string(args[0]).ok()?;
            let j_arg1 = env.new_string(args[1]).ok()?;
            let j_arg2 = env.new_string(args[2]).ok()?;
            let j_arg3 = env.new_string(args[3]).ok()?;

            let result = match env.call_static_method(
                class,
                method_name,
                "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[
                    JValue::Object(&context),
                    JValue::Object(&j_arg0),
                    JValue::Object(&j_arg1),
                    JValue::Object(&j_arg2),
                    JValue::Object(&j_arg3),
                ],
            ) {
                Ok(v) => match v.l() {
                    Ok(l) => l,
                    Err(e) => {
                        log::error!("NIP-55 JNI: {}.l() failed: {}", method_name, e);
                        return None;
                    }
                },
                Err(e) => {
                    log::error!("NIP-55 JNI: {} call failed: {}", method_name, e);
                    return None;
                }
            };

            if result.is_null() {
                return None;
            }

            let ret = match env.get_string((&result).into()) {
                Ok(jstr) => Some(jstr.to_string_lossy().into_owned()),
                Err(e) => {
                    log::error!("NIP-55 JNI: get_string failed in {}: {}", method_name, e);
                    None
                }
            };
            ret
        }
        _ => {
            log::error!(
                "NIP-55 JNI: unexpected arg count {} for {}",
                args.len(),
                method_name
            );
            None
        }
    }
}

/// Call a ContentResolver method that takes (Context, String signerPackage) and returns String?.
fn call_content_resolver_simple(method_name: &str, signer_package: &str) -> Option<String> {
    use jni::objects::{JObject, JValue};

    let ctx = ndk_context::android_context();
    let vm = match unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(v) => v,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: JavaVM::from_raw failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };
    let mut env = match vm.attach_current_thread() {
        Ok(e) => e,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: attach_current_thread failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };

    let context = unsafe { JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")?;

    let j_signer_package = match env.new_string(signer_package) {
        Ok(s) => s,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: new_string('{}') failed in {}: {}",
                signer_package,
                method_name,
                e
            );
            return None;
        }
    };

    let result = match env.call_static_method(
        class,
        method_name,
        "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
        &[JValue::Object(&context), JValue::Object(&j_signer_package)],
    ) {
        Ok(v) => match v.l() {
            Ok(l) => l,
            Err(e) => {
                log::error!("NIP-55 JNI: {}.l() failed: {}", method_name, e);
                return None;
            }
        },
        Err(e) => {
            log::error!("NIP-55 JNI: {} call failed: {}", method_name, e);
            return None;
        }
    };

    if result.is_null() {
        return None;
    }

    let ret = match env.get_string((&result).into()) {
        Ok(jstr) => Some(jstr.to_string_lossy().into_owned()),
        Err(e) => {
            log::error!("NIP-55 JNI: get_string failed in {}: {}", method_name, e);
            None
        }
    };
    ret
}

/// Call a static boolean method on MainActivity companion object.
fn call_static_bool(method_name: &str) -> Option<bool> {
    use jni::objects::JValue;

    let ctx = ndk_context::android_context();
    let vm = match unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(v) => v,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: JavaVM::from_raw failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };
    let mut env = match vm.attach_current_thread() {
        Ok(e) => e,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: attach_current_thread failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };

    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")?;

    match env.call_static_method(
        class,
        method_name,
        "(Landroid/content/Context;)Z",
        &[JValue::Object(&context)],
    ) {
        Ok(v) => match v.z() {
            Ok(b) => Some(b),
            Err(e) => {
                log::error!("NIP-55 JNI: {}.z() failed: {}", method_name, e);
                None
            }
        },
        Err(e) => {
            log::error!("NIP-55 JNI: {} call failed: {}", method_name, e);
            None
        }
    }
}

/// Call a static void method on MainActivity that takes Context.
fn call_static_void(method_name: &str) {
    use jni::objects::JValue;

    let ctx = ndk_context::android_context();
    let vm = match unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(v) => v,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: JavaVM::from_raw failed in {}: {}",
                method_name,
                e
            );
            return;
        }
    };
    let mut env = match vm.attach_current_thread() {
        Ok(e) => e,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: attach_current_thread failed in {}: {}",
                method_name,
                e
            );
            return;
        }
    };

    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = match find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity") {
        Some(c) => c,
        None => return,
    };

    match env.call_static_method(
        class,
        method_name,
        "(Landroid/content/Context;)V",
        &[JValue::Object(&context)],
    ) {
        Ok(_) => {}
        Err(e) => {
            log::error!("NIP-55 JNI: {} call failed: {}", method_name, e);
        }
    }
}

/// Call a ContentResolver-based static method on MainActivity companion object.
///
/// For sign_event: args = [eventJson, currentUser]
/// For encrypt/decrypt: args = [content, pubkey, currentUser]
fn call_content_resolver(method_name: &str, args: &[&str], signer_package: &str) -> Option<String> {
    use jni::objects::{JObject, JValue};

    let ctx = ndk_context::android_context();
    let vm = match unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(v) => v,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: JavaVM::from_raw failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };
    let mut env = match vm.attach_current_thread() {
        Ok(e) => e,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: attach_current_thread failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };

    let context = unsafe { JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")?;

    let j_signer_package = match env.new_string(signer_package) {
        Ok(s) => s,
        Err(e) => {
            log::error!(
                "NIP-55 JNI: new_string signer_package failed in {}: {}",
                method_name,
                e
            );
            return None;
        }
    };

    // Build JNI signature and arguments based on method
    match args.len() {
        // sign_event: (Context, String signerPackage, String eventJson, String currentUser) -> String?
        2 => {
            let j_arg0 = match env.new_string(args[0]) {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "NIP-55 JNI: new_string arg0 failed in {}: {}",
                        method_name,
                        e
                    );
                    return None;
                }
            };
            let j_arg1 = match env.new_string(args[1]) {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "NIP-55 JNI: new_string arg1 failed in {}: {}",
                        method_name,
                        e
                    );
                    return None;
                }
            };

            let result = match env.call_static_method(
                class,
                method_name,
                "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[
                    JValue::Object(&context),
                    JValue::Object(&j_signer_package),
                    JValue::Object(&j_arg0),
                    JValue::Object(&j_arg1),
                ],
            ) {
                Ok(v) => match v.l() {
                    Ok(l) => l,
                    Err(e) => {
                        log::error!("NIP-55 JNI: {}.l() failed: {}", method_name, e);
                        return None;
                    }
                },
                Err(e) => {
                    log::error!("NIP-55 JNI: {} call failed: {}", method_name, e);
                    return None;
                }
            };

            if result.is_null() {
                return None;
            }

            let ret = match env.get_string((&result).into()) {
                Ok(jstr) => Some(jstr.to_string_lossy().into_owned()),
                Err(e) => {
                    log::error!("NIP-55 JNI: get_string failed in {}: {}", method_name, e);
                    None
                }
            };
            ret
        }
        // encrypt/decrypt: (Context, String signerPackage, String content, String pubkey, String currentUser) -> String?
        3 => {
            let j_arg0 = match env.new_string(args[0]) {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "NIP-55 JNI: new_string arg0 failed in {}: {}",
                        method_name,
                        e
                    );
                    return None;
                }
            };
            let j_arg1 = match env.new_string(args[1]) {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "NIP-55 JNI: new_string arg1 failed in {}: {}",
                        method_name,
                        e
                    );
                    return None;
                }
            };
            let j_arg2 = match env.new_string(args[2]) {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "NIP-55 JNI: new_string arg2 failed in {}: {}",
                        method_name,
                        e
                    );
                    return None;
                }
            };

            let result = match env.call_static_method(
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
            ) {
                Ok(v) => match v.l() {
                    Ok(l) => l,
                    Err(e) => {
                        log::error!("NIP-55 JNI: {}.l() failed: {}", method_name, e);
                        return None;
                    }
                },
                Err(e) => {
                    log::error!("NIP-55 JNI: {} call failed: {}", method_name, e);
                    return None;
                }
            };

            if result.is_null() {
                return None;
            }

            let ret = match env.get_string((&result).into()) {
                Ok(jstr) => Some(jstr.to_string_lossy().into_owned()),
                Err(e) => {
                    log::error!("NIP-55 JNI: get_string failed in {}: {}", method_name, e);
                    None
                }
            };
            ret
        }
        _ => {
            log::error!(
                "NIP-55 JNI: unexpected arg count {} for {}",
                args.len(),
                method_name
            );
            None
        }
    }
}
