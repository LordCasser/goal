//! OS window integration. No planner data or AI tool depends on this layer.
#[cfg(any(target_os = "windows", test))]
pub(crate) mod geometry;
#[cfg(target_os = "windows")]
pub(crate) mod windows;

#[cfg(target_os = "windows")]
mod windows_frame;
