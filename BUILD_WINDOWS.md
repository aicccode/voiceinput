# Windows 构建指南

## 前置条件

### 1. 安装 Rust
```powershell
# 下载并运行 rustup-init.exe
# https://rustup.rs/
# 安装完成后重启终端，确认：
rustc --version
cargo --version
```

### 2. 安装 Visual Studio Build Tools
- 下载: https://visualstudio.microsoft.com/zh-hans/visual-cpp-build-tools/
- 安装时勾选 **"使用 C++ 的桌面开发"** 工作负载
- 确保包含: MSVC v143+、Windows 10/11 SDK、CMake

### 3. 安装 WebView2 Runtime（Windows 10 1903+ 通常已自带）
- 下载: https://developer.microsoft.com/en-us/microsoft-edge/webview2/

### 4. 安装 LLVM/Clang（whisper.cpp 编译需要）
```powershell
winget install LLVM.LLVM
# 或从 https://github.com/llvm/llvm-project/releases 下载
# 安装后确保 clang 在 PATH 中：
clang --version
```

## 构建步骤

### 方式一：开发模式（推荐先测试）

不嵌入模型，需要外部模型文件，编译快。

```powershell
cd voiceinput\src-tauri

# 构建
cargo build

# 运行前，把模型文件放在 exe 旁边或上级目录
# 模型会自动搜索: exe目录/ggml-small.bin, exe目录/ggml-base.bin
copy ..\ggml-base.bin target\debug\

# 运行
target\debug\voiceinput.exe
```

### 方式二：发布模式（单文件分发）

模型嵌入 exe，体积约 500MB，首次运行自动解压到缓存目录。

```powershell
cd voiceinput\src-tauri

# 确保 ggml-small.bin 在项目根目录（voiceinput/ggml-small.bin）
# 然后构建 release 版本
cargo build --release

# 生成的单文件 exe：
# target\release\voiceinput.exe  （约 500MB）
```

## 模型文件

| 模型 | 大小 | 精度 | 下载 |
|------|------|------|------|
| ggml-base.bin | ~141MB | 一般 | 适合开发测试 |
| ggml-small.bin | ~466MB | 较好 | 推荐正式使用 |

下载地址（任选一个）：
- https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
- https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
- 国内镜像: https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main/ggml-small.bin

## 运行方式

### 开发模式运行
```
voiceinput/
├── src-tauri/target/debug/
│   ├── voiceinput.exe
│   └── ggml-base.bin      ← 模型文件放这里
```

### 发布模式分发
只需要一个文件：`voiceinput.exe`
- 首次启动会解压模型到 `%LOCALAPPDATA%\voiceinput\models\`
- 后续启动直接读取缓存，秒开

## 使用方法

1. 双击运行 `voiceinput.exe`
2. 系统托盘出现麦克风图标
3. **按住右 Ctrl 键** → 弹出录音窗口
4. **说话** → 实时显示声音波形
5. **松开右 Ctrl** → 自动识别并复制到剪贴板
6. 在任意位置 **Ctrl+V** 粘贴
