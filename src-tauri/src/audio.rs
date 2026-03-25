use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use parking_lot::Mutex;
use std::sync::Arc;

const SAMPLE_RATE: u32 = 16000;
const MAX_DURATION_SECS: usize = 60;

/// Wraps cpal::Stream to satisfy Send requirement.
///
/// cpal::Stream is !Send on some platforms because the underlying audio API
/// handles are not thread-safe. We protect all access through the outer
/// Mutex<AudioRecorder> in AppData, so sending the whole AudioRecorder
/// across threads (needed for Arc<AppData>) is safe.
#[allow(dead_code)] // Field kept for RAII: dropping it stops the audio stream
struct SendStream(Option<Stream>);

// SAFETY: AudioRecorder is only ever accessed via Mutex<AudioRecorder>,
// guaranteeing exclusive access. The Stream is never shared across threads.
unsafe impl Send for SendStream {}

pub struct AudioRecorder {
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: SendStream,
    rms_callback: Arc<Mutex<Option<Box<dyn Fn(f32) + Send + 'static>>>>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::with_capacity(SAMPLE_RATE as usize * 10))),
            stream: SendStream(None),
            rms_callback: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_rms_callback<F: Fn(f32) + Send + 'static>(&self, callback: F) {
        *self.rms_callback.lock() = Some(Box::new(callback));
    }

    pub fn start(&mut self) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "未检测到麦克风设备".to_string())?;

        log::info!("Using input device: {}", device.name().unwrap_or_default());

        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer = self.buffer.clone();
        let rms_cb = self.rms_callback.clone();
        let max_samples = SAMPLE_RATE as usize * MAX_DURATION_SECS;

        // Clear previous buffer
        buffer.lock().clear();

        let err_fn = |err: cpal::StreamError| {
            log::error!("Audio stream error: {}", err);
        };

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut buf = buffer.lock();
                    if buf.len() < max_samples {
                        buf.extend_from_slice(data);

                        // Calculate RMS for waveform visualization
                        if !data.is_empty() {
                            let rms = (data.iter().map(|s| s * s).sum::<f32>()
                                / data.len() as f32)
                                .sqrt();
                            if let Some(ref cb) = *rms_cb.lock() {
                                cb(rms);
                            }
                        }
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("Failed to build audio stream: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {}", e))?;

        self.stream = SendStream(Some(stream));
        log::info!("Audio recording started");
        Ok(())
    }

    pub fn stop(&mut self) -> Vec<f32> {
        // Drop the stream to stop recording
        self.stream = SendStream(None);
        log::info!("Audio recording stopped");

        let buffer = self.buffer.lock().clone();
        log::info!(
            "Recorded {} samples ({:.1}s)",
            buffer.len(),
            buffer.len() as f32 / SAMPLE_RATE as f32
        );
        buffer
    }
}
