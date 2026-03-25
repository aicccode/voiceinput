# VoiceInput

**离线语音转文字输入工具。按住右 Ctrl，说话，松开——文字自动复制到剪贴板。**

[English](README.md)

---

<p align="center">
  <strong>按住右 Ctrl → 说话 → 松开 → 随处粘贴</strong>
</p>

## 特性

- **本地推理** — Whisper AI 模型完全在本地运行，首次启动需联网下载模型（约 466MB），此后无需联网，数据不出设备
- **轻量分发** — 可执行文件仅约 15MB，下载即用，模型首次运行时自动下载（支持断点续传）
- **全局热键** — 在任何应用中按住右 Ctrl 即可语音输入，无需切换窗口
- **跨平台** — 支持 Linux、Windows、macOS
- **实时波形** — 录音时实时显示声音波形
- **中文优化** — 针对中文语音优化，支持中英混合，自动补全标点
- **系统托盘** — 后台静默运行，仅显示托盘图标
- **断点续传** — 模型下载中断后，重启自动继续

## 使用流程

```
按住右Ctrl  →  说话  →  松开右Ctrl  →  Ctrl+V 粘贴
     ┌────────────────────────────────────┐
     │  正在聆听... (实时波形显示)          │
     │  录音时长: 3.2s                     │
     └────────────────────────────────────┘
              ↓ 松开按键
     ┌────────────────────────────────────┐
     │  正在识别...                        │
     └────────────────────────────────────┘
              ↓ 识别完成
     ┌────────────────────────────────────┐
     │  你好世界这是语音输入测试            │
     │  已复制到剪贴板                     │
     └────────────────────────────────────┘
```

## 快速开始

### 下载

前往 [Releases](../../releases) 下载对应平台的最新版本：

| 平台 | 文件 | 说明 |
|------|------|------|
| Linux x86_64 | `voiceinput` | 需要 PulseAudio/PipeWire |
| Windows x86_64 | `voiceinput.exe` | 需要 WebView2（Win10+ 已自带） |
| macOS ARM | `voiceinput` | 需要辅助功能权限 |

### 运行

```bash
# Linux / macOS
chmod +x voiceinput
./voiceinput

# Windows — 直接双击
voiceinput.exe
```

首次启动会自动下载 Whisper 语音模型（约 466MB），下载支持断点续传。

- **中国用户**：自动检测系统语言，从 `hf-mirror.com` 下载（国内镜像，速度快）
- **海外用户**：从 `huggingface.co` 下载
- **手动指定**：在配置文件中设置 `mirror = "cn"` 或 `mirror = "global"`

### 使用

1. 系统托盘出现麦克风图标
2. **按住右 Ctrl** — 弹出录音浮窗，开始说话
3. **松开右 Ctrl** — 自动识别语音
4. 识别结果自动复制到剪贴板 — 在任意位置 **Ctrl+V** 粘贴

## 配置文件

首次运行自动生成，路径：
- Linux: `~/.config/voiceinput/config.toml`
- Windows: `%APPDATA%\voiceinput\config.toml`
- macOS: `~/Library/Application Support/voiceinput/config.toml`

```toml
[hotkey]
trigger = "RControl"       # 触发键
min_hold_ms = 300          # 最短按住时间（毫秒）

[audio]
sample_rate = 16000        # 采样率（Whisper 要求 16kHz）
max_duration_sec = 60      # 最长录音时间（秒）
min_duration_ms = 500      # 最短录音时间（毫秒）

[whisper]
language = "zh"            # 识别语言: zh, en, ja 等
beam_size = 5              # Beam search 宽度
threads = 0                # 推理线程数，0 = 自动

[general]
log_level = "info"         # 日志级别: debug, info, warn, error
mirror = "auto"            # 模型下载镜像: "auto"(自动), "cn"(国内镜像), "global"(国际)
```

## 从源码构建

### 前置条件

| 工具 | 所有平台 | 说明 |
|------|---------|------|
| Rust | [rustup.rs](https://rustup.rs/) | 稳定版工具链 |
| LLVM/Clang | 必需 | 编译 whisper.cpp 需要 |

**Linux 额外依赖：**
```bash
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev \
  librsvg2-dev libasound2-dev libxdo-dev
```

**Windows 额外依赖：** 安装 Visual Studio Build Tools，勾选"使用 C++ 的桌面开发"。

### 构建

```bash
git clone https://github.com/YOUR_USERNAME/voiceinput.git
cd voiceinput

# 开发构建（首次运行自动下载模型）
cd src-tauri && cargo build

# 发布构建
cd src-tauri && cargo build --release
```

### GitHub Actions 自动构建

推送 tag 即可自动构建三平台：

```bash
git tag v1.0.0
git push origin v1.0.0
# → GitHub Actions 自动构建 Linux/Windows/macOS
# → 产物自动发布到 GitHub Releases
```

也可以在仓库 Actions 页面手动触发构建。

## 技术栈

| 模块 | 技术 |
|------|------|
| 框架 | Tauri 2.0（Rust 后端 + WebView 前端） |
| 语音识别 | whisper.cpp（通过 whisper-rs） |
| 音频采集 | cpal（ALSA/PulseAudio/WASAPI/CoreAudio） |
| 全局热键 | rdev（全局键盘钩子） |
| 剪贴板 | arboard |
| HTTP | ureq（模型下载） |
| 模型 | Whisper Small（约 466MB，MIT 许可证） |

## 系统要求

| 项目 | 最低配置 |
|------|---------|
| CPU | 4 核，支持 AVX2 指令集（2013 年后的 Intel/AMD） |
| 内存 | 8 GB |
| 硬盘 | 500 MB 可用空间 |
| 网络 | 首次启动需要联网下载模型 |
| 麦克风 | 任意音频输入设备 |
| GPU | 不需要（纯 CPU 推理） |

## 许可证

MIT
