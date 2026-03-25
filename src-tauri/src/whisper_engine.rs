use std::path::PathBuf;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Minimum expected file sizes for each model type (bytes).
/// These are conservative lower bounds for valid GGML whisper models.
const MIN_MODEL_SIZES: &[(&str, u64)] = &[
    ("tiny", 70_000_000),    // ~75 MB
    ("base", 140_000_000),   // ~141 MB
    ("small", 450_000_000),  // ~466 MB
    ("medium", 1_400_000_000), // ~1.5 GB
    ("large", 2_800_000_000),  // ~3 GB
];

pub struct WhisperEngine {
    ctx: WhisperContext,
}

impl WhisperEngine {
    /// Initialize whisper engine from model file path
    pub fn new(model_path: &str) -> Result<Self, String> {
        log::info!("Loading whisper model from: {}", model_path);

        // Validate model file before loading to prevent segfault on truncated files
        validate_model_file(model_path)?;

        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(model_path, params)
            .map_err(|e| format!("Failed to load whisper model: {}", e))?;
        log::info!("Whisper model loaded successfully");
        Ok(Self { ctx })
    }

    /// Transcribe PCM f32 audio data to text
    pub fn transcribe_with_config(
        &self,
        audio: &[f32],
        language: &str,
        beam_size: i32,
        threads_config: i32,
    ) -> Result<String, String> {
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size,
            patience: 1.0,
        });

        params.set_language(Some(language));
        params.set_translate(false);
        params.set_no_timestamps(true);
        params.set_single_segment(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_temperature(0.0);

        // Set number of threads (0 = auto)
        let threads = if threads_config > 0 {
            threads_config
        } else {
            let num_cpus = std::thread::available_parallelism()
                .map(|n| n.get() as i32)
                .unwrap_or(4);
            (num_cpus - 1).max(1)
        };
        params.set_n_threads(threads);
        log::info!("Whisper using {} threads", threads);

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| format!("Failed to create whisper state: {}", e))?;

        let start = std::time::Instant::now();
        state
            .full(params, audio)
            .map_err(|e| format!("Whisper transcription failed: {}", e))?;
        let elapsed = start.elapsed();
        log::info!("Transcription took {:.2}s", elapsed.as_secs_f32());

        let num_segments = state.full_n_segments()
            .map_err(|e| format!("Failed to get segments: {}", e))?;
        let mut text = String::new();

        for i in 0..num_segments {
            if let Ok(segment) = state.full_get_segment_text(i) {
                text.push_str(&segment);
            }
        }

        let text = text.trim().to_string();
        log::info!("Transcribed: \"{}\"", text);
        Ok(text)
    }
}

/// Validate that a GGML model file is complete and not truncated.
/// Reads the header to determine model type, then checks file size against
/// minimum expected size. Prevents segfaults from loading truncated models.
fn validate_model_file(path: &str) -> Result<(), String> {
    use std::fs;
    use std::io::Read;

    let metadata = fs::metadata(path)
        .map_err(|e| format!("Cannot read model file '{}': {}", path, e))?;
    let file_size = metadata.len();

    // Read GGML header to determine model type
    let mut file = fs::File::open(path)
        .map_err(|e| format!("Cannot open model file: {}", e))?;

    let mut header = [0u8; 28]; // magic(4) + n_vocab(4) + n_audio_ctx(4) + n_audio_state(4) + n_audio_head(4) + n_audio_layer(4) + n_text_ctx(4)
    file.read_exact(&mut header)
        .map_err(|e| format!("Cannot read model header: {}", e))?;

    // Verify GGML magic number (0x67676d6c in little-endian)
    let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if magic != 0x67676d6c {
        return Err(format!(
            "Invalid model file: bad magic number 0x{:08x} (expected 0x67676d6c)",
            magic
        ));
    }

    // Determine model type from n_audio_layer (offset 20)
    let n_audio_layer = u32::from_le_bytes([header[20], header[21], header[22], header[23]]);
    let model_type = match n_audio_layer {
        4 => "tiny",
        6 => "base",
        12 => "small",
        24 => "medium",
        32 => "large",
        _ => {
            log::warn!("Unknown model type (n_audio_layer={}), skipping size validation", n_audio_layer);
            return Ok(());
        }
    };

    // Check file size against minimum for this model type
    if let Some((_, min_size)) = MIN_MODEL_SIZES.iter().find(|(name, _)| *name == model_type) {
        if file_size < *min_size {
            return Err(format!(
                "Model file appears truncated: {} is {} bytes ({:.1} MB) but {} model should be at least {:.1} MB. \
                 Please re-download the model.",
                path,
                file_size,
                file_size as f64 / 1_048_576.0,
                model_type,
                *min_size as f64 / 1_048_576.0,
            ));
        }
    }

    log::info!(
        "Model validated: type={}, size={:.1} MB",
        model_type,
        file_size as f64 / 1_048_576.0
    );
    Ok(())
}

/// Get the model cache path for the current platform
pub fn model_cache_path() -> PathBuf {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("voiceinput")
        .join("models");
    cache_dir.join("ggml-small.bin")
}

/// Ensure model file exists at cache path.
/// In embed mode: extract from binary if not cached.
/// In dev mode: look for model file in project directory or env var.
#[allow(unused_variables)]
pub fn ensure_model(project_dir: Option<&str>) -> Result<String, String> {
    let cache_path = model_cache_path();

    // If cached model exists, validate and use it
    if cache_path.exists() {
        let path_str = cache_path.to_string_lossy().to_string();
        match validate_model_file(&path_str) {
            Ok(()) => {
                log::info!("Using cached model: {}", cache_path.display());
                return Ok(path_str);
            }
            Err(e) => {
                log::warn!("Cached model invalid, removing: {}", e);
                let _ = std::fs::remove_file(&cache_path);
            }
        }
    }

    #[cfg(embed_model)]
    {
        use std::fs;
        // Extract embedded model to cache
        log::info!("Extracting embedded model to: {}", cache_path.display());
        let model_data: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../ggml-small.bin"));

        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create cache dir: {}", e))?;
        }

        fs::write(&cache_path, model_data)
            .map_err(|e| format!("Failed to write model to cache: {}", e))?;

        log::info!("Model extracted ({} bytes)", model_data.len());
        return Ok(cache_path.to_string_lossy().to_string());
    }

    #[cfg(dev_mode)]
    {
        // Dev mode: try env var, then project directory
        if let Ok(path) = std::env::var("VOICEINPUT_MODEL_PATH") {
            if std::path::Path::new(&path).exists() {
                match validate_model_file(&path) {
                    Ok(()) => {
                        log::info!("Dev mode: using model from env: {}", path);
                        return Ok(path);
                    }
                    Err(e) => {
                        log::warn!("Model from env invalid: {}", e);
                    }
                }
            }
        }

        // Try project directory - search for models in priority order
        let names = ["ggml-small.bin", "ggml-base.bin"];
        let mut candidates: Vec<PathBuf> = Vec::new();
        for name in &names {
            if let Some(dir) = project_dir {
                candidates.push(PathBuf::from(dir).join(name));
            }
            candidates.push(PathBuf::from(name));
            candidates.push(PathBuf::from("..").join(name));
        }

        for candidate in &candidates {
            if candidate.exists() {
                let path = candidate.to_string_lossy().to_string();
                match validate_model_file(&path) {
                    Ok(()) => {
                        log::info!("Dev mode: using model from: {}", path);
                        return Ok(path);
                    }
                    Err(e) => {
                        log::warn!("Skipping invalid model {}: {}", path, e);
                    }
                }
            }
        }

        Err("No valid model file found. Set VOICEINPUT_MODEL_PATH or place ggml-small.bin / ggml-base.bin in project root.".to_string())
    }

    #[cfg(not(any(embed_model, dev_mode)))]
    {
        Err("No model available. Build with embed-model feature or set VOICEINPUT_DEV=1".to_string())
    }
}
