/// Get the current time as seconds since Unix epoch.
pub fn now_secs() -> u64 {
    #[cfg(feature = "web")]
    {
        (js_sys::Date::now() / 1000.0) as u64
    }
    #[cfg(not(feature = "web"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// Get the current time as milliseconds since Unix epoch.
pub fn now_millis() -> u64 {
    #[cfg(feature = "web")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(feature = "web"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
