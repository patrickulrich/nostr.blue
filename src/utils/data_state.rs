/// Type-state machine for async data operations
///
/// Replaces multiple boolean flags (is_loading, has_error) with a single enum
/// that makes impossible states unrepresentable. This pattern ensures that
/// data can only be in one state at a time and provides better type safety.
///
/// # Examples
///
/// ```
/// // Instead of:
/// let mut is_loading = use_signal(|| false);
/// let mut data = use_signal(|| None);
/// let mut error = use_signal(|| None);
///
/// // Use:
/// let mut state = use_signal(|| DataState::Pending);
///
/// // During fetch:
/// state.set(DataState::Loading);
///
/// // On success:
/// state.set(DataState::Loaded(data));
///
/// // On error:
/// state.set(DataState::Error("Failed to load".to_string()));
/// ```

#[derive(Debug, Clone, PartialEq)]
pub enum DataState<T> {
    /// Initial state, no action taken yet
    Pending,

    /// Currently loading/fetching data
    Loading,

    /// Successfully loaded with data
    Loaded(T),

    /// Failed to load with error message
    Error(String),
}

impl<T> DataState<T> {
    /// Returns true if state is Pending
    pub fn is_pending(&self) -> bool {
        matches!(self, DataState::Pending)
    }

    /// Returns true if state is Loading
    pub fn is_loading(&self) -> bool {
        matches!(self, DataState::Loading)
    }

    /// Returns true if state is Loaded
    pub fn is_loaded(&self) -> bool {
        matches!(self, DataState::Loaded(_))
    }

    /// Returns true if state is Error
    pub fn is_error(&self) -> bool {
        matches!(self, DataState::Error(_))
    }

    /// Returns the data if loaded, None otherwise
    pub fn data(&self) -> Option<&T> {
        match self {
            DataState::Loaded(data) => Some(data),
            _ => None,
        }
    }

    /// Returns the error message if in error state, None otherwise
    pub fn error(&self) -> Option<&str> {
        match self {
            DataState::Error(msg) => Some(msg),
            _ => None,
        }
    }

    /// Consumes self and returns the data if loaded, None otherwise
    pub fn into_data(self) -> Option<T> {
        match self {
            DataState::Loaded(data) => Some(data),
            _ => None,
        }
    }

    /// Maps the data using a function if in Loaded state
    pub fn map<U, F>(self, f: F) -> DataState<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            DataState::Pending => DataState::Pending,
            DataState::Loading => DataState::Loading,
            DataState::Loaded(data) => DataState::Loaded(f(data)),
            DataState::Error(msg) => DataState::Error(msg),
        }
    }

    /// Maps the data using a function that returns a Result
    pub fn and_then<U, F>(self, f: F) -> DataState<U>
    where
        F: FnOnce(T) -> Result<U, String>,
    {
        match self {
            DataState::Pending => DataState::Pending,
            DataState::Loading => DataState::Loading,
            DataState::Loaded(data) => match f(data) {
                Ok(new_data) => DataState::Loaded(new_data),
                Err(err) => DataState::Error(err),
            },
            DataState::Error(msg) => DataState::Error(msg),
        }
    }
}

impl<T> Default for DataState<T> {
    fn default() -> Self {
        DataState::Pending
    }
}

/// Helper to convert Result into DataState
impl<T, E: std::fmt::Display> From<Result<T, E>> for DataState<T> {
    fn from(result: Result<T, E>) -> Self {
        match result {
            Ok(data) => DataState::Loaded(data),
            Err(err) => DataState::Error(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transitions() {
        let state: DataState<i32> = DataState::Pending;
        assert!(state.is_pending());

        let state = DataState::Loading;
        assert!(state.is_loading());

        let state = DataState::Loaded(42);
        assert!(state.is_loaded());
        assert_eq!(state.data(), Some(&42));

        let state = DataState::Error("test error".to_string());
        assert!(state.is_error());
        assert_eq!(state.error(), Some("test error"));
    }

    #[test]
    fn test_map() {
        let state = DataState::Loaded(42);
        let mapped = state.map(|x| x * 2);
        assert_eq!(mapped.data(), Some(&84));

        let state: DataState<i32> = DataState::Pending;
        let mapped = state.map(|x| x * 2);
        assert!(mapped.is_pending());
    }

    #[test]
    fn test_and_then() {
        let state = DataState::Loaded(42);
        let result = state.and_then(|x| Ok(x * 2));
        assert_eq!(result.data(), Some(&84));

        let state = DataState::Loaded(42);
        let result = state.and_then(|_| Err::<i32, _>("error".to_string()));
        assert!(result.is_error());
    }

    #[test]
    fn test_from_result() {
        let result: Result<i32, String> = Ok(42);
        let state: DataState<i32> = result.into();
        assert_eq!(state.data(), Some(&42));

        let result: Result<i32, String> = Err("error".to_string());
        let state: DataState<i32> = result.into();
        assert!(state.is_error());
    }
}
