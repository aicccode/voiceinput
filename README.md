# VoiceInput

**Offline voice-to-text input tool. Hold Right Ctrl, speak, release — text copied to clipboard.**

[中文文档](README_CN.md)

---

<p align="center">
  <strong>Hold Right Ctrl → Speak → Release → Paste anywhere</strong>
</p>

## Features

- **100% Offline** — Whisper AI model runs locally. No internet, no cloud, no data leaves your machine.
- **Single File** — One executable (~500MB with embedded model). Download, run, done.
- **Global Hotkey** — Works in any application. No window switching needed.
- **Cross-Platform** — Linux, Windows, macOS.
- **Real-time Waveform** — Visual feedback while recording.
- **Chinese Optimized** — Tuned for Chinese speech with punctuation post-processing. English works too.
- **System Tray** — Runs silently in the background. No taskbar clutter.

## How It Works

```
Hold Right Ctrl  →  Speak  →  Release Right Ctrl  →  Ctrl+V to paste
     ┌────────────────────────────────────┐
     │  🎙️ Recording... (waveform shown)  │
     │  "正在聆听..."                      │
     └────────────────────────────────────┘
              ↓ release key
     ┌────────────────────────────────────┐
     │  ⏳ Transcribing...                │
     └────────────────────────────────────┘
              ↓ done
     ┌────────────────────────────────────┐
     │  ✅ 你好世界这是语音输入测试         │
     │  Copied to clipboard               │
     └────────────────────────────────────┘
```

## Quick Start

### Download

Go to [Releases](../../releases) and download the latest binary for your platform:

| Platform | File | Notes |
|----------|------|-------|
| Linux x86_64 | `voiceinput` | Requires PulseAudio/PipeWire |
| Windows x86_64 | `voiceinput.exe` | Requires WebView2 (pre-installed on Win10+) |
| macOS ARM | `voiceinput` | Requires Accessibility permission |

### Run

```bash
# Linux / macOS — make executable and run
chmod +x voiceinput
./voiceinput

# Windows — just double-click
voiceinput.exe
```

On first launch, the embedded model extracts to a local cache (~466MB, one-time only). Subsequent launches are instant.

### Use

1. A microphone icon appears in the system tray
2. **Hold Right Ctrl** — overlay appears, start speaking
3. **Release Right Ctrl** — transcription begins
4. Result is copied to clipboard — **Ctrl+V** to paste anywhere

## Configuration

Config file is auto-created at:
- Linux: `~/.config/voiceinput/config.toml`
- Windows: `%APPDATA%\voiceinput\config.toml`
- macOS: `~/Library/Application Support/voiceinput/config.toml`

```toml
[hotkey]
trigger = "RControl"       # Trigger key
min_hold_ms = 300          # Minimum hold to activate (ms)

[audio]
sample_rate = 16000        # Hz (Whisper requirement)
max_duration_sec = 60      # Auto-stop after this
min_duration_ms = 500      # Ignore if shorter

[whisper]
language = "zh"            # "zh", "en", "ja", etc.
beam_size = 5              # Beam search width
threads = 0                # 0 = auto (CPU cores - 1)

[general]
log_level = "info"         # debug, info, warn, error
```

## Build from Source

### Prerequisites

| Tool | All Platforms | Notes |
|------|---------------|-------|
| Rust | [rustup.rs](https://rustup.rs/) | Stable toolchain |
| LLVM/Clang | Required | For whisper.cpp compilation |

**Linux extras:**
```bash
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev \
  librsvg2-dev libasound2-dev libxdo-dev
```

**Windows extras:** Visual Studio Build Tools with "Desktop C++" workload.

### Build

```bash
git clone https://github.com/YOUR_USERNAME/voiceinput.git
cd voiceinput

# Download a whisper model
curl -LO https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin

# Dev build (model loaded from file, fast compile)
cd src-tauri && cargo build

# Release build (model embedded in binary)
cd src-tauri && cargo build --release
```

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Framework | Tauri 2.0 (Rust + WebView) |
| Speech-to-Text | whisper.cpp via whisper-rs |
| Audio Capture | cpal (ALSA/PulseAudio/WASAPI/CoreAudio) |
| Hotkey | rdev (global keyboard hook) |
| Clipboard | arboard |
| Model | Whisper Small (~466MB, MIT license) |

## System Requirements

| | Minimum |
|---|---------|
| CPU | 4 cores, AVX2 support (2013+ Intel/AMD) |
| RAM | 8 GB |
| Disk | 500 MB free |
| Microphone | Any audio input device |
| GPU | Not required |

## License

MIT
