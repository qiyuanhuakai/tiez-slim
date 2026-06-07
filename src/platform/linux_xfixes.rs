//! XFixes-based clipboard monitoring for Linux/X11.
//!
//! Uses the XFixes extension for efficient clipboard change notification
//! instead of polling, reducing CPU usage on X11 desktops.
//!
//! // TODO(T4): Implement XFixes clipboard selection monitoring (Wave 1)
//! // TODO(T17): Implement XFixes-based paste integration (Wave 2)

#[cfg(target_os = "linux")]
pub fn is_available() -> bool {
    // Placeholder — will be populated in T4.
    false
}
