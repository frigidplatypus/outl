//! Capture inbox command (RFC 0363). Its own macro because `catalog.rs`
//! sits at its size ceiling; only the mobile client registers it.

/// `drain_capture_inbox`: land the iOS extensions' captures.
#[macro_export]
macro_rules! capture_commands {
    ($state:ty) => {
        $crate::tauri_commands! {
            state = $state;

            /// Land every capture waiting in the App Group inbox.
            fn drain_capture_inbox()
                -> ::std::result::Result<$crate::capture_inbox::CaptureDrainDto, String>
                => $crate::capture_inbox::drain_capture_inbox;
        }
    };
}
