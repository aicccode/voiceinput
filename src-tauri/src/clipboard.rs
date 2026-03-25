use arboard::Clipboard;
use std::sync::Mutex;
use std::thread;

#[cfg(target_os = "linux")]
use arboard::SetExtLinux;

/// Manages clipboard operations.
///
/// On Linux (X11/Wayland), the clipboard is "owned" by the writing process and
/// that process must stay alive to serve paste requests. We use arboard's
/// `SetExtLinux::wait()` to block a background thread until the clipboard is
/// claimed by another application.
///
/// On Windows/macOS, the OS copies clipboard data into system memory, so no
/// background thread is needed.
pub struct ClipboardManager {
    /// Handle to background thread keeping Linux clipboard alive
    _keeper: Mutex<Option<thread::JoinHandle<()>>>,
}

impl ClipboardManager {
    pub fn new() -> Self {
        Self {
            _keeper: Mutex::new(None),
        }
    }

    /// Copy text to system clipboard.
    pub fn copy_to_clipboard(&self, text: &str) -> Result<(), String> {
        let text = text.to_string();

        #[cfg(target_os = "linux")]
        {
            // Linux: clipboard ownership must be held alive in a background thread.
            // wait() blocks until another app claims the clipboard or the timeout.
            let handle = thread::spawn(move || {
                match Clipboard::new() {
                    Ok(mut cb) => {
                        match cb.set().wait().text(&text) {
                            Ok(()) => {
                                log::info!(
                                    "Clipboard: {} chars copied, holding ownership...",
                                    text.len()
                                );
                            }
                            Err(e) => {
                                log::error!("Failed to set clipboard text: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to open clipboard: {}", e);
                    }
                }
            });

            if let Ok(mut keeper) = self._keeper.lock() {
                *keeper = Some(handle);
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            // Windows/macOS: OS persists clipboard data, no background thread needed.
            let mut cb = Clipboard::new()
                .map_err(|e| format!("Failed to open clipboard: {}", e))?;
            cb.set_text(&text)
                .map_err(|e| format!("Failed to set clipboard text: {}", e))?;
            log::info!("Clipboard: {} chars copied", text.len());
        }

        Ok(())
    }
}
