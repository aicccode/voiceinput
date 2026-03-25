use rdev::{listen, Event, EventType, Key};
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
                    // Any other key while RCtrl is held = combo, cancel recording
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
