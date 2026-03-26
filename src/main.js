// VoiceInput Frontend - Overlay UI

const { listen } = window.__TAURI__.event;
const { invoke } = window.__TAURI__.core;

// DOM Elements
const overlay = document.getElementById('overlay');
const statusText = document.getElementById('status-text');
const resultText = document.getElementById('result-text');
const durationEl = document.getElementById('duration');
const waveformCanvas = document.getElementById('waveform');
const ctx = waveformCanvas.getContext('2d');
const progressBar = document.getElementById('progress-bar');
const progressText = document.getElementById('progress-text');
const pathSelect = document.getElementById('path-select');
const selectPathBtn = document.getElementById('select-path-btn');
const defaultPathBtn = document.getElementById('default-path-btn');

// Waveform state
const WAVEFORM_BARS = 40;
const waveformData = new Array(WAVEFORM_BARS).fill(0);
let recordingStartTime = null;
let durationInterval = null;
let animationFrame = null;

// ---- Event listeners ----

listen('state-change', (event) => {
    updateState(event.payload);
});

listen('audio-level', (event) => {
    pushWaveformData(event.payload);
});

listen('transcription-result', (event) => {
    const text = event.payload;
    resultText.textContent = text;
    updateState('result');
});

listen('clipboard-ready', (event) => {
    statusText.textContent = '已复制到剪贴板';
});

listen('error', (event) => {
    statusText.textContent = event.payload;
    updateState('error');
});

// ---- Download progress ----

listen('download-progress', (event) => {
    const { downloaded, total, speed_bps, stage } = event.payload;

    if (stage === 'downloading' && total > 0) {
        const pct = (downloaded / total * 100).toFixed(1);
        progressBar.style.width = pct + '%';

        const downloadedMB = (downloaded / 1048576).toFixed(1);
        const totalMB = (total / 1048576).toFixed(1);
        const speedMBs = (speed_bps / 1048576).toFixed(1);
        progressText.textContent = downloadedMB + ' / ' + totalMB + ' MB  (' + speedMBs + ' MB/s)';
    } else if (stage === 'verifying') {
        statusText.textContent = '正在验证模型...';
        progressBar.style.width = '100%';
        progressText.textContent = '';
    }
});

listen('download-error', (event) => {
    statusText.textContent = '下载失败: ' + event.payload;
    progressText.textContent = '请检查网络后重启应用';
    updateState('error');
});

listen('model-ready', () => {
    updateState('ready');
});

listen('need-select-path', (event) => {
    updateState('select-path');
    if (event.payload) {
        progressText.textContent = '默认路径: ' + event.payload;
    }
});

// ---- Path selection buttons ----

selectPathBtn.addEventListener('click', async () => {
    selectPathBtn.disabled = true;
    defaultPathBtn.disabled = true;
    statusText.textContent = '正在打开文件选择器...';

    try {
        const path = await invoke('cmd_select_model_path');
        if (path) {
            statusText.textContent = '准备下载...';
        } else {
            // User cancelled
            statusText.textContent = '首次运行，请选择模型保存路径';
            selectPathBtn.disabled = false;
            defaultPathBtn.disabled = false;
        }
    } catch (e) {
        statusText.textContent = '选择路径失败: ' + e;
        selectPathBtn.disabled = false;
        defaultPathBtn.disabled = false;
    }
});

defaultPathBtn.addEventListener('click', async () => {
    selectPathBtn.disabled = true;
    defaultPathBtn.disabled = true;
    statusText.textContent = '准备下载...';

    try {
        await invoke('cmd_use_default_model_path');
    } catch (e) {
        statusText.textContent = '操作失败: ' + e;
        selectPathBtn.disabled = false;
        defaultPathBtn.disabled = false;
    }
});

// ---- State machine ----

function updateState(state) {
    overlay.className = 'overlay visible';

    switch (state) {
        case 'select-path':
            overlay.classList.add('state-select-path');
            statusText.textContent = '首次运行，请选择模型保存路径';
            resultText.textContent = '';
            progressBar.style.width = '0%';
            durationEl.textContent = '';
            resetWaveform();
            selectPathBtn.disabled = false;
            defaultPathBtn.disabled = false;
            break;

        case 'downloading':
            overlay.classList.add('state-downloading');
            statusText.textContent = '正在下载语音模型...';
            resultText.textContent = '';
            progressText.textContent = '准备下载...';
            progressBar.style.width = '0%';
            durationEl.textContent = '';
            resetWaveform();
            break;

        case 'loading':
            overlay.classList.add('state-loading');
            statusText.textContent = '正在加载模型...';
            resultText.textContent = '';
            progressText.textContent = '';
            progressBar.style.width = '0%';
            durationEl.textContent = '';
            resetWaveform();
            startLoadingAnimation();
            break;

        case 'ready':
            overlay.classList.add('state-ready');
            statusText.textContent = '模型加载完成';
            resultText.textContent = '';
            progressText.textContent = '按住右 Ctrl 开始说话';
            progressBar.style.width = '100%';
            durationEl.textContent = '';
            stopLoadingAnimation();
            resetWaveform();
            break;

        case 'waiting':
            overlay.classList.add('state-waiting');
            statusText.textContent = '请开始说话...';
            resultText.textContent = '';
            progressText.textContent = '';
            durationEl.textContent = '';
            resetWaveform();
            break;

        case 'recording':
            overlay.classList.add('state-recording');
            statusText.textContent = '正在聆听...';
            resultText.textContent = '';
            progressText.textContent = '';
            startDurationTimer();
            startWaveformAnimation();
            break;

        case 'transcribing':
            overlay.classList.add('state-transcribing');
            statusText.textContent = '正在识别...';
            resultText.textContent = '';
            progressText.textContent = '';
            stopDurationTimer();
            stopWaveformAnimation();
            durationEl.textContent = '';
            break;

        case 'result':
            overlay.classList.add('state-result');
            statusText.textContent = '识别完成';
            progressText.textContent = '';
            stopDurationTimer();
            stopWaveformAnimation();
            durationEl.textContent = '';
            break;

        case 'idle':
            stopDurationTimer();
            stopWaveformAnimation();
            resetWaveform();
            break;

        case 'error':
            overlay.classList.add('state-error');
            stopDurationTimer();
            stopWaveformAnimation();
            progressText.textContent = '';
            durationEl.textContent = '';
            break;
    }
}

// ---- Duration timer ----

function startDurationTimer() {
    recordingStartTime = Date.now();
    stopDurationTimer();
    durationInterval = setInterval(() => {
        const elapsed = (Date.now() - recordingStartTime) / 1000;
        durationEl.textContent = elapsed.toFixed(1) + 's';
    }, 100);
}

function stopDurationTimer() {
    if (durationInterval) {
        clearInterval(durationInterval);
        durationInterval = null;
    }
}

// ---- Waveform visualization ----

function pushWaveformData(rms) {
    const normalized = Math.min(1, rms * 5);
    waveformData.shift();
    waveformData.push(normalized);
}

function resetWaveform() {
    waveformData.fill(0);
    ctx.clearRect(0, 0, waveformCanvas.width, waveformCanvas.height);
}

function drawWaveform() {
    const w = waveformCanvas.width;
    const h = waveformCanvas.height;
    const barW = w / WAVEFORM_BARS - 2;
    const cy = h / 2;

    ctx.clearRect(0, 0, w, h);

    for (let i = 0; i < WAVEFORM_BARS; i++) {
        const v = waveformData[i];
        const barH = Math.max(2, v * cy * 0.9);
        const x = i * (barW + 2) + 1;

        const hue = 200 - v * 80;
        const sat = 70 + v * 30;
        const lit = 50 + v * 15;

        ctx.fillStyle = 'hsla(' + hue + ',' + sat + '%,' + lit + '%,0.9)';

        const y = cy - barH;
        const bh = barH * 2;
        if (ctx.roundRect) {
            ctx.beginPath();
            ctx.roundRect(x, y, barW, bh, 2);
            ctx.fill();
        } else {
            ctx.fillRect(x, y, barW, bh);
        }
    }
}

function startWaveformAnimation() {
    stopWaveformAnimation();
    function animate() {
        drawWaveform();
        animationFrame = requestAnimationFrame(animate);
    }
    animate();
}

function stopWaveformAnimation() {
    if (animationFrame) {
        cancelAnimationFrame(animationFrame);
        animationFrame = null;
    }
}

// ---- Loading animation (indeterminate progress) ----

let loadingInterval = null;
let loadingDir = 1;
let loadingPos = 0;

function startLoadingAnimation() {
    stopLoadingAnimation();
    loadingPos = 0;
    loadingDir = 1;
    loadingInterval = setInterval(() => {
        loadingPos += loadingDir * 2;
        if (loadingPos >= 90) loadingDir = -1;
        if (loadingPos <= 0) loadingDir = 1;
        progressBar.style.width = (10 + loadingPos) + '%';
    }, 50);
}

function stopLoadingAnimation() {
    if (loadingInterval) {
        clearInterval(loadingInterval);
        loadingInterval = null;
    }
}

// ---- Init ----
updateState('idle');
