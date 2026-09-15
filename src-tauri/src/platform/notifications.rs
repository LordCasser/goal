//! Synchronous desktop notification submission.
//!
//! Tauri's desktop notification plugin reports a successful asynchronous
//! spawn even when the native notification server later rejects the request.
//! This boundary deliberately uses notify-rust's synchronous `show` call so
//! the reminder service can distinguish an OS submission failure from an
//! accepted request.  It still cannot promise that the user saw the toast;
//! the reminder remains in the in-app fired list for that case.

#[cfg(any(target_os = "macos", test))]
use std::sync::OnceLock;

/// The stable identity used by native notification grouping.
pub(crate) const APP_ID: &str = "dev.lordcasser.planner";

/// Desktop permission APIs are not exposed by notify-rust.  The OS owns this
/// setting, so the UI must not display a fabricated `granted` value.
pub(crate) const SYSTEM_MANAGED_PERMISSION: &str = "system_managed";

#[cfg(target_os = "macos")]
static MAC_APPLICATION: OnceLock<Result<(), String>> = OnceLock::new();

pub(crate) fn permission_state() -> &'static str {
    SYSTEM_MANAGED_PERMISSION
}

pub(crate) fn request_permission() -> &'static str {
    SYSTEM_MANAGED_PERMISSION
}

#[cfg(any(target_os = "macos", test))]
fn initialize_once<T: Clone>(slot: &OnceLock<T>, init: impl FnOnce() -> T) -> T {
    slot.get_or_init(init).clone()
}

/// Submit one notification to the native desktop server and report only the
/// synchronous submission result.  This does not claim that the user saw it.
pub(crate) fn send(title: &str, body: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    initialize_once(&MAC_APPLICATION, || {
        notify_rust::set_application(APP_ID)
            .map_err(|error| format!("macOS notification identity unavailable: {error}"))
    })?;

    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body);

    #[cfg(target_os = "windows")]
    notification.app_id(APP_ID);

    #[cfg(all(unix, not(target_os = "macos")))]
    notification.appname(APP_ID);

    notification
        .show()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::{initialize_once, permission_state, request_permission, SYSTEM_MANAGED_PERMISSION};

    #[test]
    fn desktop_permission_is_explicitly_system_managed() {
        assert_eq!(permission_state(), SYSTEM_MANAGED_PERMISSION);
        assert_eq!(request_permission(), SYSTEM_MANAGED_PERMISSION);
        assert_ne!(permission_state(), "granted");
    }

    #[test]
    fn identity_initialization_keeps_the_first_result() {
        let slot = OnceLock::new();
        assert_eq!(
            initialize_once(&slot, || Err::<(), _>("first failure".to_string())),
            Err("first failure".to_string())
        );
        assert_eq!(
            initialize_once(&slot, || Ok::<(), String>(())),
            Err("first failure".to_string())
        );
    }

    #[test]
    fn identity_initialization_does_not_run_a_second_initializer() {
        let slot = OnceLock::new();
        assert_eq!(initialize_once(&slot, || Ok::<(), String>(())), Ok(()));
        assert_eq!(
            initialize_once(&slot, || -> Result<(), String> {
                panic!("second initializer must not run")
            }),
            Ok(())
        );
    }
}
