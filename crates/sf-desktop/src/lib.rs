pub mod capabilities;
pub mod commands;
pub mod content;
pub mod deep_link;
pub mod engine_bridge;
pub mod error;
pub mod installer;
pub mod license;
pub mod signing;
pub mod updater;
pub mod vault;
pub mod watcher;


#[cfg(test)]
#[track_caller]
pub(crate) fn test_result<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("Expected successful test operation, got: {error:?}"),
    }
}
