// VoiceInput Frontend - Overlay UI

const { listen } = window.__TAURI__.event;

// DOM Elements
const overlay = document.getElementById('overlay');
const statusText = document.getElementById('status-text');
const resultText = document.getElementById('result-text');
const durationEl = document.getElementById('duration');
const waveformCanvas = document.getElementById('waveform');
const ctx = waveformCanvas.getContext('2d');

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

// Transcription result text — show it in the overlay
listen('transcription-result', (event) => {
    const text = event.payload;
    resultText.textContent = text;
    updateState('result');
});

// Clipboard copied — update status text, then close after 1s
listen('clipboard-ready', (event) => {
    statusText.textContent = '已复制到剪贴板';
});

listen('error', (event) => {
    statusText.textContent = event.payload;
    updateState('error');
});

// ---- State machine ----

function updateState(state) {
    overlay.className = 'overlay visible';

    switch (state) {
        case 'waiting':
            overlay.classList.add('state-waiting');
            statusText.textContent = '请开始说话...';
            resultText.textContent = '';
            durationEl.textContent = '';
            resetWaveform();
            break;

        case 'recording':
            overlay.classList.add('state-recording');
            statusText.textContent = '正在聆听...';
            resultText.textContent = '';
            startDurationTimer();
            startWaveformAnimation();
            break;

        case 'transcribing':
            overlay.classList.add('state-transcribing');
            statusText.textContent = '正在识别...';
            resultText.textContent = '';
            stopDurationTimer();
            stopWaveformAnimation();
            durationEl.textContent = '';
            break;

        case 'result':
            overlay.classList.add('state-result');
            statusText.textContent = '识别完成';
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

        // Blue (quiet) → green (loud)
        const hue = 200 - v * 80;
        const sat = 70 + v * 30;
        const lit = 50 + v * 15;

        ctx.fillStyle = 'hsla(' + hue + ',' + sat + '%,' + lit + '%,0.9)';

        // Fallback: use fillRect if roundRect is unavailable (older WebView)
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

// ---- Init ----
updateState('waiting');
