use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::Deref;
use zeroize::Zeroize;

/// A `String` wrapper that zeroizes its contents on drop.
///
/// Used for sensitive material like mnemonics and private keys that should
/// not linger in heap memory after the owning struct is dropped.
#[derive(Clone, PartialEq)]
pub struct ZeroizeString(pub String);

impl Deref for ZeroizeString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for ZeroizeString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Serialize for ZeroizeString {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ZeroizeString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(ZeroizeString(String::deserialize(deserializer)?))
    }
}

impl std::fmt::Debug for ZeroizeString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}
