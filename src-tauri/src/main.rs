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
    /// User-selected model directory (empty = use default cache dir)
    custom_model_dir: Mutex<String>,
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
        custom_model_dir: Mutex::new(cfg.general.model_path.clone()),
        config: cfg.clone(),
        state: AppStateManager::new(),
        recorder: Mutex::new(AudioRecorder::new()),
        whisper: OnceLock::new(),
        clipboard: ClipboardManager::new(),
        recording_session: AtomicU64::new(0),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_data.clone())
        .invoke_handler(tauri::generate_handler![
            cmd_select_model_path,
            cmd_use_default_model_path,
        ])
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

/// Check model and either load directly, download, or ask user to select path.
fn init_model(handle: AppHandle, data: Arc<AppData>) {
    let model_dir = data.custom_model_dir.lock().clone();
    let cache_path = whisper_engine::model_cache_path_custom(&model_dir);
    let model_ready = whisper_engine::model_is_ready_at(&cache_path);

    if model_ready {
        // Model exists, load directly
        log::info!("Loading model from cache...");
        let _ = handle.emit("state-change", "loading");
        show_overlay(&handle);
        load_model(&handle, &data, &cache_path);
        return;
    }

    if model_dir.is_empty() {
        // No custom path configured, ask user to select
        log::info!("No model path configured, asking user to select");
        let default_dir = whisper_engine::default_model_dir().to_string_lossy().to_string();
        let _ = handle.emit("need-select-path", &default_dir);
        show_overlay(&handle);
        return;
    }

    // Custom path configured but model not found, download to it
    log::info!("Model not found at {:?}, starting download...", cache_path);
    let _ = handle.emit("state-change", "downloading");
    show_overlay(&handle);
    download_and_load(&handle, &data, &cache_path);
}

/// Download model to the given path, then load it.
fn download_and_load(
    handle: &AppHandle,
    data: &Arc<AppData>,
    cache_path: &std::path::Path,
) {
    let url = model_downloader::get_download_url(&data.config.general.mirror);
    let handle_progress = handle.clone();

    let result = model_downloader::download_model(cache_path, url, move |progress| {
        let _ = handle_progress.emit("download-progress", &progress);
    });

    if let Err(e) = result {
        log::error!("Model download failed: {}", e);
        let _ = handle.emit("download-error", &e);
        return;
    }

    if !whisper_engine::model_is_ready_at(cache_path) {
        log::error!("Downloaded model is invalid");
        let _ = std::fs::remove_file(cache_path);
        let _ = handle.emit(
            "download-error",
            "Downloaded model is invalid, please restart to retry",
        );
        return;
    }

    load_model(handle, data, cache_path);
}

/// Load an existing model file into the whisper engine.
fn load_model(handle: &AppHandle, data: &Arc<AppData>, cache_path: &std::path::Path) {
    log::info!("Loading whisper model...");
    let _ = handle.emit("state-change", "loading");

    let model_path = cache_path.to_string_lossy().to_string();
    match whisper_engine::WhisperEngine::new(&model_path) {
        Ok(engine) => {
            let _ = data.whisper.set(engine);
            log::info!("Whisper model loaded and ready");
            let _ = handle.emit("model-ready", true);
            std::thread::sleep(std::time::Duration::from_secs(3));
            hide_overlay(handle);
        }
        Err(e) => {
            log::error!("Failed to load whisper model: {}", e);
            let _ = handle.emit("download-error", format!("模型加载失败: {}", e));
        }
    }
}

/// Tauri command: open native folder picker for model save path.
#[tauri::command]
fn cmd_select_model_path(
    app: AppHandle,
    data: tauri::State<'_, Arc<AppData>>,
) -> Result<Option<String>, String> {
    let default_dir = whisper_engine::default_model_dir();
    let _ = std::fs::create_dir_all(&default_dir);

    let folder = rfd::FileDialog::new()
        .set_title("选择模型保存路径")
        .set_directory(&default_dir)
        .pick_folder();

    let folder = match folder {
        Some(p) => p,
        None => return Ok(None), // User cancelled
    };

    let path_str = folder.to_string_lossy().to_string();
    log::info!("User selected model path: {}", path_str);

    // Save to runtime state
    *data.custom_model_dir.lock() = path_str.clone();

    // Persist to config file
    let mut cfg = config::load_config();
    cfg.general.model_path = path_str.clone();
    if let Err(e) = config::save_config(&cfg) {
        log::warn!("Failed to save config: {}", e);
    }

    // Start download/load in background
    let handle = app.clone();
    let data_clone = data.inner().clone();
    std::thread::spawn(move || {
        let cache_path = whisper_engine::model_cache_path_custom(&path_str);
        if whisper_engine::model_is_ready_at(&cache_path) {
            load_model(&handle, &data_clone, &cache_path);
        } else {
            let _ = handle.emit("state-change", "downloading");
            download_and_load(&handle, &data_clone, &cache_path);
        }
    });

    Ok(Some(folder.to_string_lossy().to_string()))
}

/// Tauri command: use the default cache directory for model storage.
#[tauri::command]
fn cmd_use_default_model_path(
    app: AppHandle,
    data: tauri::State<'_, Arc<AppData>>,
) -> Result<String, String> {
    let default_dir = whisper_engine::default_model_dir();
    let dir_str = default_dir.to_string_lossy().to_string();
    log::info!("Using default model path: {}", dir_str);

    // Clear custom path
    *data.custom_model_dir.lock() = String::new();

    // Save to config
    let mut cfg = config::load_config();
    cfg.general.model_path = String::new();
    if let Err(e) = config::save_config(&cfg) {
        log::warn!("Failed to save config: {}", e);
    }

    // Start download/load in background
    let handle = app.clone();
    let data_clone = data.inner().clone();
    std::thread::spawn(move || {
        let cache_path = whisper_engine::model_cache_path();
        if whisper_engine::model_is_ready_at(&cache_path) {
            load_model(&handle, &data_clone, &cache_path);
        } else {
            let _ = handle.emit("state-change", "downloading");
            download_and_load(&handle, &data_clone, &cache_path);
        }
    });

    Ok(dir_str)
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
