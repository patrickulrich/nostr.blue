//! Mostro toast emission helper.
//!
//! Mostro actions produce 0..N toasts each. `apply_mostro_action` returns
//! a `Vec<MostroToast>`; callers (the trade-detail page, the background
//! monitor) emit them via `emit_toasts(&toasts)`.
//!
//! This centralizes toast content alongside status transitions so the
//! state machine has a single source of truth.

use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MostroToastKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug)]
pub struct MostroToast {
    pub kind: MostroToastKind,
    pub title: String,
    pub body: Option<String>,
    pub duration: Option<Duration>,
}

impl MostroToast {
    pub fn info(title: impl Into<String>) -> Self {
        Self {
            kind: MostroToastKind::Info,
            title: title.into(),
            body: None,
            duration: None,
        }
    }

    #[allow(dead_code)]
    pub fn success(title: impl Into<String>) -> Self {
        Self {
            kind: MostroToastKind::Success,
            title: title.into(),
            body: None,
            duration: None,
        }
    }

    pub fn warning(title: impl Into<String>) -> Self {
        Self {
            kind: MostroToastKind::Warning,
            title: title.into(),
            body: None,
            duration: None,
        }
    }

    pub fn error(title: impl Into<String>) -> Self {
        Self {
            kind: MostroToastKind::Error,
            title: title.into(),
            body: None,
            duration: None,
        }
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn duration(mut self, d: Duration) -> Self {
        self.duration = Some(d);
        self
    }
}

/// Emit a slice of `MostroToast` via the dioxus-primitives toast API.
///
/// No-op in non-UI contexts (e.g. tests) — `consume_toast` requires a
/// Dioxus runtime; callers in test contexts should not invoke this.
pub fn emit_toasts(toasts: &[MostroToast]) {
    let api = dioxus_primitives::toast::consume_toast();
    for t in toasts {
        let mut opts = dioxus_primitives::toast::ToastOptions::new();
        if let Some(body) = &t.body {
            opts = opts.description(body.clone());
        }
        if let Some(d) = t.duration {
            opts = opts.duration(d);
        }
        match t.kind {
            MostroToastKind::Info => api.info(t.title.clone(), opts),
            MostroToastKind::Success => api.success(t.title.clone(), opts),
            MostroToastKind::Warning => api.warning(t.title.clone(), opts),
            MostroToastKind::Error => api.error(t.title.clone(), opts),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_constructors() {
        let t = MostroToast::info("hello").body("world");
        assert_eq!(t.kind, MostroToastKind::Info);
        assert_eq!(t.title, "hello");
        assert_eq!(t.body.as_deref(), Some("world"));
        assert!(t.duration.is_none());

        let t = MostroToast::warning("warn");
        assert_eq!(t.kind, MostroToastKind::Warning);
        assert!(t.body.is_none());

        let t = MostroToast::error("err").duration(Duration::from_secs(5));
        assert_eq!(t.kind, MostroToastKind::Error);
        assert_eq!(t.duration, Some(Duration::from_secs(5)));
    }
}
