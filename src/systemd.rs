//! Systemd integration via `sd_notify`
//! See <https://www.freedesktop.org/software/systemd/man/latest/sd_notify.html>

/// Sends `READY=1` to systemd via sd_notify. No-op if not Type=notify.
pub fn sd_notify_ready() {
    if let Err(error) = sd_notify::notify(&[sd_notify::NotifyState::Ready]) {
        warn!(message = "Failed to notify systemd of ready state.", %error);
    }
}

/// Sends `STOPPING=1` to systemd via sd_notify. No-op if not Type=notify.
pub fn sd_notify_stopping() {
    if let Err(error) = sd_notify::notify(&[sd_notify::NotifyState::Stopping]) {
        warn!(message = "Failed to notify systemd of stopping state.", %error);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sd_notify_no_socket_does_not_panic() {
        // NOTIFY_SOCKET is not set in test environments - these must be no-ops.
        super::sd_notify_ready();
        super::sd_notify_stopping();
    }
}
