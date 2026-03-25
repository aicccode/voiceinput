use std::path::PathBuf;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Minimum expected file sizes for each model type (bytes).
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
fn validate_model_file(path: &str) -> Result<(), String> {
    use std::fs;
    use std::io::Read;

    let metadata = fs::metadata(path)
        .map_err(|e| format!("Cannot read model file '{}': {}", path, e))?;
    let file_size = metadata.len();

    let mut file = fs::File::open(path)
        .map_err(|e| format!("Cannot open model file: {}", e))?;

    let mut header = [0u8; 28];
    file.read_exact(&mut header)
        .map_err(|e| format!("Cannot read model header: {}", e))?;

    let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if magic != 0x67676d6c {
        return Err(format!(
            "Invalid model file: bad magic number 0x{:08x} (expected 0x67676d6c)",
            magic
        ));
    }

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

/// Check if the cached model exists and is valid
pub fn model_is_ready() -> bool {
    let path = model_cache_path();
    if !path.exists() {
        return false;
    }
    validate_model_file(&path.to_string_lossy()).is_ok()
}
