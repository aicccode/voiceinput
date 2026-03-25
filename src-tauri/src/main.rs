#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod clipboard;
mod config;
mod hotkey;
mod punctuation;
mod state;
mod tray;
mod whisper_engine;

use audio::AudioRecorder;
use clipboard::ClipboardManager;
use config::AppConfig;
use parking_lot::Mutex;
use state::{AppState, AppStateManager};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

struct AppData {
    config: AppConfig,
    state: AppStateManager,
    recorder: Mutex<AudioRecorder>,
    whisper: Option<whisper_engine::WhisperEngine>,
    clipboard: ClipboardManager,
    /// Monotonically increasing recording session ID for timeout cancellation
    recording_session: AtomicU64,
}

fn main() {
    // Load config first
    let cfg = config::load_config();

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(&cfg.general.log_level),
    )
    .init();

    log::info!("VoiceInput starting...");
    log::info!("Config: {:?}", config::config_path());

    // Resolve project root directory for model search.
    // Use the executable's directory so model lookup works regardless of CWD.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_string_lossy().to_string()));
    let project_dir = exe_dir.as_deref();

    // Ensure model is available
    let model_path = match whisper_engine::ensure_model(project_dir) {
        Ok(path) => path,
        Err(e) => {
            log::error!("Model not available: {}", e);
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    // Load whisper model
    log::info!("Loading whisper model...");
    let whisper = match whisper_engine::WhisperEngine::new(&model_path) {
        Ok(w) => {
            log::info!("Whisper model loaded successfully");
            Some(w)
        }
        Err(e) => {
            log::error!("Failed to load whisper model: {}", e);
            eprintln!("Warning: Whisper model failed to load: {}", e);
            None
        }
    };

    let app_data = Arc::new(AppData {
        config: cfg.clone(),
        state: AppStateManager::new(),
        recorder: Mutex::new(AudioRecorder::new()),
        whisper,
        clipboard: ClipboardManager::new(),
        recording_session: AtomicU64::new(0),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            let data = app_data.clone();

            // Build tray menu
            tray::setup_tray(&handle)?;

            // Start global hotkey listener with configured debounce
            let hotkey_rx = hotkey::start_listener(data.config.hotkey.min_hold_ms);

            // Hotkey event loop
            let handle2 = handle.clone();
            std::thread::spawn(move || {
                hotkey_event_loop(handle2, data, hotkey_rx);
            });

            // Hide main window on startup (tray-only mode)
            if let Some(window) = handle.get_webview_window("overlay") {
                let _ = window.hide();
            }

            log::info!("VoiceInput ready");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn hotkey_event_loop(
    handle: AppHandle,
    data: Arc<AppData>,
    rx: std::sync::mpsc::Receiver<hotkey::HotkeyEvent>,
) {
    for event in rx {
        match event {
            hotkey::HotkeyEvent::Pressed => {
                handle_key_press(&handle, &data);
            }
            hotkey::HotkeyEvent::Released => {
                handle_key_release(&handle, &data);
            }
            hotkey::HotkeyEvent::Cancelled => {
                handle_key_cancel(&handle, &data);
            }
        }
    }
}

fn handle_key_press(handle: &AppHandle, data: &Arc<AppData>) {
    if !data.state.is_idle() {
        return;
    }

    // Show overlay window
    data.state.set(AppState::Waiting);
    let _ = handle.emit("state-change", "waiting");
    show_overlay(handle);

    // Start audio recording
    let handle_clone = handle.clone();
    let mut recorder = data.recorder.lock();

    // Set up RMS callback for waveform
    recorder.set_rms_callback(move |rms| {
        let _ = handle_clone.emit("audio-level", rms);
    });

    match recorder.start() {
        Ok(_) => {
            data.state.set(AppState::Recording);
            let _ = handle.emit("state-change", "recording");
            tray::update_tray_icon(handle, tray::TrayState::Recording);

            // Start recording timeout timer
            let session_id = data.recording_session.fetch_add(1, Ordering::SeqCst) + 1;
            let timeout_secs = data.config.audio.max_duration_sec;
            let timeout_handle = handle.clone();
            let timeout_data = data.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(timeout_secs as u64));
                // Only trigger if this session is still active
                if timeout_data.recording_session.load(Ordering::SeqCst) == session_id
                    && timeout_data.state.get() == AppState::Recording
                {
                    log::warn!("Recording timeout ({}s), auto-stopping", timeout_secs);
                    process_recording(&timeout_handle, &timeout_data);
                }
            });
        }
        Err(e) => {
            log::error!("Failed to start recording: {}", e);
            data.state.set(AppState::Error);
            let _ = handle.emit("error", e);
            tray::update_tray_icon(handle, tray::TrayState::Idle);
            hide_overlay_delayed(handle);
        }
    }
}

/// Called when RCtrl is released after a valid hold (no combo detected).
fn handle_key_release(handle: &AppHandle, data: &Arc<AppData>) {
    let current_state = data.state.get();
    if current_state != AppState::Recording && current_state != AppState::Waiting {
        return;
    }
    // Invalidate timeout timer
    data.recording_session.fetch_add(1, Ordering::SeqCst);

    // Collect audio and transcribe on a worker thread
    // so the hotkey listener remains responsive.
    let audio_data = data.recorder.lock().stop();
    let handle = handle.clone();
    let data = data.clone();
    std::thread::spawn(move || {
        process_recording_with_audio(&handle, &data, audio_data);
    });
}

/// Stop recording, transcribe, and copy text to clipboard.
/// Called from the timeout path (needs to stop the recorder itself).
fn process_recording(handle: &AppHandle, data: &Arc<AppData>) {
    let audio_data = data.recorder.lock().stop();
    process_recording_with_audio(handle, data, audio_data);
}

/// Core transcription pipeline. Runs on a worker thread so the hotkey
/// listener is never blocked during the 3-8 second whisper inference.
fn process_recording_with_audio(handle: &AppHandle, data: &Arc<AppData>, audio_data: Vec<f32>) {
    let duration_ms = (audio_data.len() as u64 * 1000) / data.config.audio.sample_rate as u64;

    if duration_ms < data.config.audio.min_duration_ms {
        log::info!("Recording too short ({}ms), canceling", duration_ms);
        data.state.set(AppState::Idle);
        let _ = handle.emit("state-change", "idle");
        tray::update_tray_icon(handle, tray::TrayState::Idle);
        hide_overlay(handle);
        return;
    }

    // Check if audio has any significant content
    let max_amplitude = audio_data
        .iter()
        .map(|s| s.abs())
        .fold(0.0f32, f32::max);

    if max_amplitude < 0.01 {
        log::info!("No voice detected (max amplitude: {})", max_amplitude);
        data.state.set(AppState::Error);
        let _ = handle.emit("error", "未检测到语音");
        tray::update_tray_icon(handle, tray::TrayState::Idle);
        hide_overlay_delayed(handle);
        return;
    }

    // Start transcription
    data.state.set(AppState::Transcribing);
    let _ = handle.emit("state-change", "transcribing");

    if let Some(ref whisper) = data.whisper {
        match whisper.transcribe_with_config(
            &audio_data,
            &data.config.whisper.language,
            data.config.whisper.beam_size,
            data.config.whisper.threads,
        ) {
            Ok(text) => {
                // Filter whisper hallucinations and empty results
                let text = filter_hallucinations(&text);
                if text.is_empty() {
                    log::info!("Empty transcription result (filtered)");
                    data.state.set(AppState::Idle);
                    let _ = handle.emit("state-change", "idle");
                    tray::update_tray_icon(handle, tray::TrayState::Idle);
                    hide_overlay(handle);
                    return;
                }

                // Post-process punctuation
                let text = punctuation::fix_punctuation(&text);
                log::info!("Final text: \"{}\"", text);

                // Show transcribed text in overlay
                let _ = handle.emit("transcription-result", &text);

                // Copy text to clipboard
                match data.clipboard.copy_to_clipboard(&text) {
                    Ok(_) => {
                        log::info!("Text copied to clipboard, ready to paste");
                        // Brief pause so user sees the text before "已复制" message
                        std::thread::sleep(std::time::Duration::from_millis(800));
                        let _ = handle.emit("clipboard-ready", &text);
                    }
                    Err(e) => {
                        log::error!("Failed to copy to clipboard: {}", e);
                        let _ = handle.emit("error", "复制到剪贴板失败");
                    }
                }

                // Hold overlay so user can read, then dismiss
                std::thread::sleep(std::time::Duration::from_millis(1200));
            }
            Err(e) => {
                log::error!("Transcription error: {}", e);
                let _ = handle.emit("error", format!("识别失败: {}", e));
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    } else {
        log::error!("Whisper engine not available");
        let _ = handle.emit("error", "语音引擎未加载");
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    data.state.set(AppState::Idle);
    let _ = handle.emit("state-change", "idle");
    tray::update_tray_icon(handle, tray::TrayState::Idle);
    hide_overlay(handle);
}

/// Filter out common whisper hallucination patterns that appear on silence or noise.
fn filter_hallucinations(text: &str) -> String {
    let trimmed = text.trim();
    // Whisper produces these when input is silence/noise
    let hallucination_patterns = [
        "[BLANK_AUDIO]",
        "(music)",
        "(Music)",
        "[音乐]",
        "[音樂]",
        "(字幕由",
        "字幕由",
        "请不吝点赞",
        "谢谢观看",
        "感谢观看",
        "订阅",
        "Thank you for watching",
        "Thanks for watching",
        "Subtitles by",
        "ご視聴ありがとうございました",
    ];
    for pattern in &hallucination_patterns {
        if trimmed.contains(pattern) || trimmed == *pattern {
            log::info!("Filtered hallucination: \"{}\"", trimmed);
            return String::new();
        }
    }
    trimmed.to_string()
}

/// Called when RCtrl release is cancelled (combo key pressed or hold too short).
fn handle_key_cancel(handle: &AppHandle, data: &Arc<AppData>) {
    let current_state = data.state.get();
    if current_state != AppState::Recording && current_state != AppState::Waiting {
        return;
    }

    log::info!("Recording cancelled (combo or short press)");
    // Stop recording and discard audio
    data.recorder.lock().stop();
    data.state.set(AppState::Idle);
    let _ = handle.emit("state-change", "idle");
    tray::update_tray_icon(handle, tray::TrayState::Idle);
    hide_overlay(handle);
}

fn show_overlay(handle: &AppHandle) {
    match handle.get_webview_window("overlay") {
        Some(window) => {
            log::info!("Overlay: showing window (size={:?}, pos={:?})",
                window.outer_size(), window.outer_position());
            let _ = window.center();
            let _ = window.show();
            let _ = window.set_always_on_top(true);
            let _ = window.set_focus();
            log::info!("Overlay: visible={:?}, focused={:?}",
                window.is_visible(), window.is_focused());
        }
        None => {
            log::error!("Overlay window 'overlay' not found!");
        }
    }
}

fn hide_overlay(handle: &AppHandle) {
    if let Some(window) = handle.get_webview_window("overlay") {
        let _ = window.hide();
    }
}

fn hide_overlay_delayed(handle: &AppHandle) {
    let handle = handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        hide_overlay(&handle);
    });
}
