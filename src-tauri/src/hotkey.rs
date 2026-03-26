use std::sync::mpsc;
use std::thread;
use std::time::Instant;

#[derive(Debug, Clone)]
pub enum HotkeyEvent {
    /// Right Ctrl pressed (start recording)
    Pressed,
    /// Right Ctrl released after valid hold (stop recording and transcribe)
    Released,
    /// Right Ctrl released but cancelled (combo key or too short)
    Cancelled,
}

/// Start listening for global keyboard events.
/// Returns a receiver that emits HotkeyEvents for right Ctrl key.
///
/// Implements:
/// - Combo detection: if any other key is pressed while RCtrl is held, cancel
/// - Debounce: must hold for at least `min_hold_ms` before release triggers
pub fn start_listener(min_hold_ms: u64) -> mpsc::Receiver<HotkeyEvent> {
    #[cfg(target_os = "linux")]
    {
        start_listener_evdev(min_hold_ms)
    }
    #[cfg(not(target_os = "linux"))]
    {
        start_listener_rdev(min_hold_ms)
    }
}

/// Linux: use evdev to read /dev/input directly (works on both X11 and Wayland)
#[cfg(target_os = "linux")]
fn start_listener_evdev(min_hold_ms: u64) -> mpsc::Receiver<HotkeyEvent> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        // Channel for raw key events from all keyboard device threads
        let (raw_tx, raw_rx) = mpsc::channel::<(evdev::Key, bool)>();

        // Find all keyboard devices that support RightCtrl
        let keyboards: Vec<_> = evdev::enumerate()
            .filter(|(_path, dev)| {
                dev.supported_keys()
                    .map(|keys| keys.contains(evdev::Key::KEY_RIGHTCTRL))
                    .unwrap_or(false)
            })
            .collect();

        if keyboards.is_empty() {
            log::error!(
                "No keyboard devices found in /dev/input/. \
                 Make sure user is in the 'input' group: sudo usermod -aG input $USER"
            );
            return;
        }

        // Spawn a reader thread per keyboard device
        for (path, mut device) in keyboards {
            let name = device.name().unwrap_or("unknown").to_string();
            log::info!("Hotkey: listening on {} ({:?})", name, path);

            let raw_tx = raw_tx.clone();
            thread::spawn(move || {
                loop {
                    match device.fetch_events() {
                        Ok(events) => {
                            for ev in events {
                                if let evdev::InputEventKind::Key(key) = ev.kind() {
                                    let is_press = ev.value() == 1;
                                    let is_release = ev.value() == 0;
                                    // value 2 = key repeat, ignore
                                    if is_press || is_release {
                                        if raw_tx.send((key, is_press)).is_err() {
                                            return; // receiver dropped
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Error reading events from {:?}: {}", path, e);
                            break;
                        }
                    }
                }
            });
        }
        // Drop our copy so the channel closes when all device threads exit
        drop(raw_tx);

        // Aggregator: same state machine as the original rdev implementation
        let mut rctrl_down = false;
        let mut rctrl_press_time: Option<Instant> = None;
        let mut combo_detected = false;

        for (key, is_press) in raw_rx {
            if is_press {
                if key == evdev::Key::KEY_RIGHTCTRL {
                    if !rctrl_down {
                        rctrl_down = true;
                        rctrl_press_time = Some(Instant::now());
                        combo_detected = false;
                        let _ = tx.send(HotkeyEvent::Pressed);
                        log::debug!("RCtrl pressed");
                    }
                } else if rctrl_down {
                    combo_detected = true;
                    log::debug!("Combo detected while RCtrl held");
                }
            } else {
                // key release
                if key == evdev::Key::KEY_RIGHTCTRL && rctrl_down {
                    rctrl_down = false;
                    let held_long_enough = rctrl_press_time
                        .map(|t| t.elapsed().as_millis() >= min_hold_ms as u128)
                        .unwrap_or(false);

                    if held_long_enough && !combo_detected {
                        let _ = tx.send(HotkeyEvent::Released);
                        log::debug!("RCtrl released (valid hold)");
                    } else {
                        if combo_detected {
                            log::debug!("RCtrl released (combo detected, canceling)");
                        } else {
                            log::debug!("RCtrl released (too short, canceling)");
                        }
                        let _ = tx.send(HotkeyEvent::Cancelled);
                    }
                    rctrl_press_time = None;
                }
            }
        }

        log::warn!("All keyboard device readers exited, hotkey listener stopped");
    });

    rx
}

/// macOS / Windows: use rdev (X11 XRecord / OS-level hooks)
#[cfg(not(target_os = "linux"))]
fn start_listener_rdev(min_hold_ms: u64) -> mpsc::Receiver<HotkeyEvent> {
    use rdev::{listen, Event, EventType, Key};

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut rctrl_down = false;
        let mut rctrl_press_time: Option<Instant> = None;
        let mut combo_detected = false;

        let callback = move |event: Event| {
            match event.event_type {
                EventType::KeyPress(Key::ControlRight) => {
                    if !rctrl_down {
                        rctrl_down = true;
                        rctrl_press_time = Some(Instant::now());
                        combo_detected = false;
                        let _ = tx.send(HotkeyEvent::Pressed);
                        log::debug!("RCtrl pressed");
                    }
                }
                EventType::KeyRelease(Key::ControlRight) => {
                    if rctrl_down {
                        rctrl_down = false;
                        let held_long_enough = rctrl_press_time
                            .map(|t| t.elapsed().as_millis() >= min_hold_ms as u128)
                            .unwrap_or(false);

                        if held_long_enough && !combo_detected {
                            let _ = tx.send(HotkeyEvent::Released);
                            log::debug!("RCtrl released (valid hold)");
                        } else {
                            if combo_detected {
                                log::debug!("RCtrl released (combo detected, canceling)");
                            } else {
                                log::debug!("RCtrl released (too short, canceling)");
                            }
                            let _ = tx.send(HotkeyEvent::Cancelled);
                        }
                        rctrl_press_time = None;
                    }
                }
                EventType::KeyPress(_) => {
                    if rctrl_down {
                        combo_detected = true;
                        log::debug!("Combo detected while RCtrl held");
                    }
                }
                _ => {}
            }
        };

        if let Err(error) = listen(callback) {
            log::error!("Hotkey listener error: {:?}", error);
        }
    });

    rx
}
