#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod clipboard;
mod config;
mod hotkey;
mod model_downloader;
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
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

struct AppData {
    config: AppConfig,
    state: AppStateManager,
    recorder: Mutex<AudioRecorder>,
    whisper: OnceLock<whisper_engine::WhisperEngine>,
    clipboard: ClipboardManager,
    /// Monotonically increasing recording session ID for timeout cancellation
    recording_session: AtomicU64,
}

fn main() {
    let cfg = config::load_config();

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(&cfg.general.log_level),
    )
    .init();

    log::info!("VoiceInput starting...");
    log::info!("Config: {:?}", config::config_path());

    let app_data = Arc::new(AppData {
        config: cfg.clone(),
        state: AppStateManager::new(),
        recorder: Mutex::new(AudioRecorder::new()),
        whisper: OnceLock::new(),
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

            // Start global hotkey listener
            let hotkey_rx = hotkey::start_listener(data.config.hotkey.min_hold_ms);
            let handle2 = handle.clone();
            let data2 = data.clone();
            std::thread::spawn(move || {
                hotkey_event_loop(handle2, data2, hotkey_rx);
            });

            // Model initialization in background thread
            let handle3 = handle.clone();
            let data3 = data.clone();
            std::thread::spawn(move || {
                init_model(handle3, data3);
            });

            // Overlay starts hidden (configured in tauri.conf.json)
            log::info!("VoiceInput ready");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Download (if needed) and load the whisper model.
/// Emits progress events to the frontend overlay.
fn init_model(handle: AppHandle, data: Arc<AppData>) {
    let cache_path = whisper_engine::model_cache_path();
    let needs_download = !whisper_engine::model_is_ready();

    if needs_download {
        log::info!("Model not found at {:?}, starting download...", cache_path);
        let _ = handle.emit("state-change", "downloading");
        show_overlay(&handle);

        let url = model_downloader::get_download_url(&data.config.general.mirror);
        let handle_progress = handle.clone();

        let result = model_downloader::download_model(&cache_path, url, move |progress| {
            let _ = handle_progress.emit("download-progress", &progress);
        });

        if let Err(e) = result {
            log::error!("Model download failed: {}", e);
            let _ = handle.emit("download-error", &e);
            return;
        }

        // Validate downloaded model
        if !whisper_engine::model_is_ready() {
            log::error!("Downloaded model is invalid");
            let _ = std::fs::remove_file(&cache_path);
            let _ = handle.emit(
                "download-error",
                "Downloaded model is invalid, please restart to retry",
            );
            return;
        }
    }

    // Load model
    log::info!("Loading whisper model...");
    if needs_download {
        let _ = handle.emit("state-change", "loading");
    }

    let model_path = cache_path.to_string_lossy().to_string();
    match whisper_engine::WhisperEngine::new(&model_path) {
        Ok(engine) => {
            let _ = data.whisper.set(engine);
            log::info!("Whisper model loaded and ready");
            let _ = handle.emit("model-ready", true);
            if needs_download {
                hide_overlay(&handle);
            }
        }
        Err(e) => {
            log::error!("Failed to load whisper model: {}", e);
            if needs_download {
                let _ = handle.emit("download-error", format!("Failed to load model: {}", e));
            }
        }
    }
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

    // Check if model is ready
    if data.whisper.get().is_none() {
        let _ = handle.emit("error", "语音模型正在加载中，请稍后再试");
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

    let audio_data = data.recorder.lock().stop();
    let handle = handle.clone();
    let data = data.clone();
    std::thread::spawn(move || {
        process_recording_with_audio(&handle, &data, audio_data);
    });
}

/// Stop recording, transcribe, and copy text to clipboard.
fn process_recording(handle: &AppHandle, data: &Arc<AppData>) {
    let audio_data = data.recorder.lock().stop();
    process_recording_with_audio(handle, data, audio_data);
}

/// Core transcription pipeline.
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

    data.state.set(AppState::Transcribing);
    let _ = handle.emit("state-change", "transcribing");

    if let Some(whisper) = data.whisper.get() {
        match whisper.transcribe_with_config(
            &audio_data,
            &data.config.whisper.language,
            data.config.whisper.beam_size,
            data.config.whisper.threads,
        ) {
            Ok(text) => {
                let text = filter_hallucinations(&text);
                if text.is_empty() {
                    log::info!("Empty transcription result (filtered)");
                    data.state.set(AppState::Idle);
                    let _ = handle.emit("state-change", "idle");
                    tray::update_tray_icon(handle, tray::TrayState::Idle);
                    hide_overlay(handle);
                    return;
                }

                let text = punctuation::fix_punctuation(&text);
                log::info!("Final text: \"{}\"", text);

                let _ = handle.emit("transcription-result", &text);

                match data.clipboard.copy_to_clipboard(&text) {
                    Ok(_) => {
                        log::info!("Text copied to clipboard, ready to paste");
                        std::thread::sleep(std::time::Duration::from_millis(800));
                        let _ = handle.emit("clipboard-ready", &text);
                    }
                    Err(e) => {
                        log::error!("Failed to copy to clipboard: {}", e);
                        let _ = handle.emit("error", "复制到剪贴板失败");
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(1200));
            }
            Err(e) => {
                log::error!("Transcription error: {}", e);
                let _ = handle.emit("error", format!("识别失败: {}", e));
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    } else {
        log::error!("Whisper engine not ready");
        let _ = handle.emit("error", "语音模型正在加载中，请稍后再试");
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    data.state.set(AppState::Idle);
    let _ = handle.emit("state-change", "idle");
    tray::update_tray_icon(handle, tray::TrayState::Idle);
    hide_overlay(handle);
}

/// Filter out common whisper hallucination patterns.
fn filter_hallucinations(text: &str) -> String {
    let trimmed = text.trim();
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
    data.recorder.lock().stop();
    data.state.set(AppState::Idle);
    let _ = handle.emit("state-change", "idle");
    tray::update_tray_icon(handle, tray::TrayState::Idle);
    hide_overlay(handle);
}

fn show_overlay(handle: &AppHandle) {
    match handle.get_webview_window("overlay") {
        Some(window) => {
            log::info!(
                "Overlay: showing window (size={:?}, pos={:?})",
                window.outer_size(),
                window.outer_position()
            );
            let _ = window.center();
            let _ = window.show();
            let _ = window.set_always_on_top(true);
            let _ = window.set_focus();
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
