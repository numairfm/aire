use notify_rust::{Notification, Timeout};

pub fn send_startup_notification() {
    std::thread::spawn(|| {
        let _ = Notification::new()
            .summary("aire is running in the system tray")
            .body("Click the aire tray icon to mirror a window or open Settings.")
            .appname("aire")
            .timeout(Timeout::Milliseconds(4000))
            .show();
    });
}
