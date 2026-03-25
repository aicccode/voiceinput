use std::fs;
use std::io::{Read, Write};
use std::path::Path;

const HF_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin";
const HF_MIRROR_URL: &str =
    "https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main/ggml-small.bin";

#[derive(Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub speed_bps: u64,
    /// "downloading" or "verifying"
    pub stage: String,
}

/// Detect if user is likely in China based on locale environment variables.
fn is_chinese_locale() -> bool {
    for var in &["LANG", "LC_ALL", "LANGUAGE"] {
        if let Ok(val) = std::env::var(var) {
            let v = val.to_lowercase();
            if v.starts_with("zh") || v.contains("cn") || v.contains("chinese") {
                return true;
            }
        }
    }
    false
}

/// Select download URL based on mirror config.
/// `mirror`: "auto" (detect locale), "cn" (hf-mirror), "global" (huggingface).
pub fn get_download_url(mirror: &str) -> &'static str {
    match mirror {
        "cn" => {
            log::info!("Mirror config: cn, using hf-mirror.com");
            HF_MIRROR_URL
        }
        "global" => {
            log::info!("Mirror config: global, using huggingface.co");
            HF_URL
        }
        _ => {
            if is_chinese_locale() {
                log::info!("Auto-detected Chinese locale, using hf-mirror.com");
                HF_MIRROR_URL
            } else {
                log::info!("Using huggingface.co");
                HF_URL
            }
        }
    }
}

/// Download model file with resume support and progress reporting.
///
/// - Saves to `<cache_path>.partial` during download, renames on completion.
/// - Sends `Range` header to resume from existing partial file.
/// - Calls `progress_cb` at most every 200ms with current progress.
pub fn download_model<F>(cache_path: &Path, url: &str, progress_cb: F) -> Result<(), String>
where
    F: Fn(DownloadProgress),
{
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create cache dir: {}", e))?;
    }

    let partial_path = cache_path.with_extension("bin.partial");

    // Check for existing partial download (resume support)
    let existing_size = if partial_path.exists() {
        fs::metadata(&partial_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    log::info!("Downloading model from: {}", url);
    if existing_size > 0 {
        log::info!("Resuming download from {} bytes", existing_size);
    }

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(300))
        .build();

    let response = if existing_size > 0 {
        agent
            .get(url)
            .set("Range", &format!("bytes={}-", existing_size))
            .call()
    } else {
        agent.get(url).call()
    }
    .map_err(|e| format!("Download request failed: {}", e))?;

    let status = response.status();
    let (total_size, resume_offset) = if status == 206 {
        // Partial content — resume accepted
        let total = response
            .header("content-range")
            .and_then(|h| h.split('/').last())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        (total, existing_size)
    } else if (200..300).contains(&status) {
        // Full response — server may not support Range, start fresh
        let total = response
            .header("content-length")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        (total, 0u64)
    } else {
        return Err(format!("Download failed with HTTP {}", status));
    };

    let mut file = if resume_offset > 0 {
        fs::OpenOptions::new()
            .append(true)
            .open(&partial_path)
            .map_err(|e| format!("Failed to open partial file: {}", e))?
    } else {
        fs::File::create(&partial_path)
            .map_err(|e| format!("Failed to create download file: {}", e))?
    };

    let mut downloaded = resume_offset;
    let mut reader = response.into_reader();
    let mut buffer = vec![0u8; 128 * 1024]; // 128 KB chunks
    let start_time = std::time::Instant::now();
    let mut last_report = std::time::Instant::now();

    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| format!("Download read error: {}", e))?;
        if n == 0 {
            break;
        }

        file.write_all(&buffer[..n])
            .map_err(|e| format!("Write error: {}", e))?;
        downloaded += n as u64;

        // Rate-limit progress updates to ~5 per second
        if last_report.elapsed().as_millis() >= 200 {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                ((downloaded - resume_offset) as f64 / elapsed) as u64
            } else {
                0
            };
            progress_cb(DownloadProgress {
                downloaded,
                total: total_size,
                speed_bps: speed,
                stage: "downloading".into(),
            });
            last_report = std::time::Instant::now();
        }
    }

    file.flush().map_err(|e| format!("Flush error: {}", e))?;
    drop(file);

    // Signal verification stage
    progress_cb(DownloadProgress {
        downloaded: total_size,
        total: total_size,
        speed_bps: 0,
        stage: "verifying".into(),
    });

    // Rename partial → final
    fs::rename(&partial_path, cache_path)
        .map_err(|e| format!("Failed to finalize downloaded model: {}", e))?;

    log::info!(
        "Model downloaded successfully: {} ({} bytes)",
        cache_path.display(),
        downloaded
    );
    Ok(())
}
