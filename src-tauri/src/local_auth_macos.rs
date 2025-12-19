// macOS-specific Touch ID authentication using LocalAuthentication framework

#[cfg(target_os = "macos")]
pub fn authenticate_with_touch_id(reason: &str) -> Result<(), String> {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSString;
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    unsafe {
        // Get LAContext class
        let la_context_class = Class::get("LAContext").ok_or("LAContext class not found")?;
        let context: *mut Object = msg_send![la_context_class, alloc];
        let context: *mut Object = msg_send![context, init];

        if context.is_null() {
            return Err("Failed to create LAContext".to_string());
        }

        // Create the reason string
        let reason_ns = NSString::alloc(nil).init_str(reason);

        // Define the policy (deviceOwnerAuthenticationWithBiometrics = 2)
        let policy: i64 = 2; // LAPolicyDeviceOwnerAuthenticationWithBiometrics

        // Check if biometrics is available
        let mut error: id = nil;
        let can_evaluate: bool = msg_send![context, canEvaluatePolicy:policy error:&mut error];

        if !can_evaluate {
            if !error.is_null() {
                let error_desc: id = msg_send![error, localizedDescription];
                let error_str: *const i8 = msg_send![error_desc, UTF8String];
                if !error_str.is_null() {
                    let error_string = std::ffi::CStr::from_ptr(error_str)
                        .to_string_lossy()
                        .to_string();
                    return Err(format!("Touch ID not available: {}", error_string));
                }
            }
            return Err("Touch ID not available on this device".to_string());
        }

        // Create a channel to wait for async completion
        let result = Arc::new(Mutex::new(None));
        let result_clone = Arc::clone(&result);

        // Create the completion block
        let block = ConcreteBlock::new(move |success: bool, error: id| {
            let mut result = result_clone.lock().unwrap();
            if success {
                *result = Some(Ok(()));
            } else if !error.is_null() {
                let error_desc: id = msg_send![error, localizedDescription];
                let error_str: *const i8 = msg_send![error_desc, UTF8String];
                if !error_str.is_null() {
                    let error_string = std::ffi::CStr::from_ptr(error_str)
                        .to_string_lossy()
                        .to_string();
                    *result = Some(Err(error_string));
                } else {
                    *result = Some(Err("Authentication failed".to_string()));
                }
            } else {
                *result = Some(Err("Authentication failed".to_string()));
            }
        });
        let block = block.copy();

        // Perform the authentication
        let _: () = msg_send![context,
            evaluatePolicy:policy
            localizedReason:reason_ns
            reply:block
        ];

        // Wait for completion (up to 60 seconds)
        for _ in 0..600 {
            std::thread::sleep(Duration::from_millis(100));
            let guard = result.lock().unwrap();
            if let Some(ref res) = *guard {
                return res.clone();
            }
        }

        Err("Touch ID authentication timed out".to_string())
    }
}

#[cfg(target_os = "macos")]
use block::ConcreteBlock;

#[cfg(not(target_os = "macos"))]
pub fn authenticate_with_touch_id(_reason: &str) -> Result<(), String> {
    // No Touch ID on non-macOS platforms
    Ok(())
}
