pub mod api_origin;
pub mod capabilities;
pub mod commands;
pub mod content;
pub mod deep_link;
pub mod device_identity;
pub mod engine_bridge;
pub mod error;
pub mod installer;
pub mod license;
pub mod local_search;
pub mod recipe_library;
pub mod signing;
pub mod studio;
pub mod updater;
pub mod vault;
pub mod watcher;
pub mod workflow_memory;

#[cfg(test)]
#[track_caller]
pub(crate) fn test_result<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("Expected successful test operation, got: {error:?}"),
    }
}
