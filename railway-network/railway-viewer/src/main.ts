// Railway Network Viewer - TypeScript
// Connects to HDDS via hdds-ws bridge (WebSocket JSON protocol).

// Types matching the Rust simulator's JSON output
interface TrainJson {
    train_id: number;
    name: string;
    color: number;
    current_segment: number;
    position: number;  // 0.0 - 1.0 within segment
    speed: number;
    direction: number; // 1 or -1
    state: number;     // TrainState enum
    next_station: number;
    delay_seconds: number;
    timestamp_us: number;
}

interface SignalJson {
    signal_id: number;
    segment_id: number;
    position: number;
    state: number; // 0=RED, 1=YELLOW, 2=GREEN
    timestamp_us: number;
}

interface TrackSegmentJson {
    segment_id: number;
    track_type: number;
    x1: number; y1: number;
    x2: number; y2: number;
    next_segment: number;
    prev_segment: number;
    length: number;
    speed_limit: number;
    has_platform: boolean;
}

interface StationJson {
    station_id: number;
    name: string;
    segment_id: number;
    position: number;
    x: number; y: number;
    platform_count: number;
}

interface NetworkStateJson {
    train_count: number;
    trains_running: number;
    trains_at_station: number;
    signals_red: number;
    signals_green: number;
    avg_delay_seconds: number;
    timestamp_us: number;
}

// hdds-ws protocol configuration
const HDDS_WS_URL = `ws://${window.location.hostname || 'localhost'}:9090/ws`;
const TOPICS = ['railway/train', 'railway/signal', 'railway/track_segment', 'railway/station', 'railway/network_state'];

// State
const segments = new Map<number, TrackSegmentJson>();
const trains = new Map<number, TrainJson>();
const signals = new Map<number, SignalJson>();
const stations = new Map<number, StationJson>();
let networkState: NetworkStateJson | null = null;

// View settings
const view = {
    showTracks: true,
    showTrains: true,
    showSignals: true,
    showStations: true,
};

// Canvas
let canvas: HTMLCanvasElement;
let ctx: CanvasRenderingContext2D;

// WebSocket
let ws: WebSocket | null = null;
let reconnectTimer: number | null = null;

// Train state names
const TRAIN_STATES = ['STOPPED', 'ACCEL', 'CRUISE', 'BRAKE', 'AT_STATION'];

// Signal colors
const SIGNAL_COLORS = ['#ff3333', '#ffcc00', '#00ff44']; // RED, YELLOW, GREEN

function init() {
    canvas = document.getElementById('map-canvas') as HTMLCanvasElement;
    ctx = canvas.getContext('2d')!;

    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);

    setupControls();
    connectWebSocket();
    requestAnimationFrame(render);
}

function resizeCanvas() {
    const container = canvas.parentElement!;
    canvas.width = container.clientWidth;
    canvas.height = container.clientHeight;
}

function setupControls() {
    document.getElementById('btn-tracks')?.addEventListener('click', (e) => {
        view.showTracks = !view.showTracks;
        (e.target as HTMLElement).classList.toggle('active', view.showTracks);
    });
    document.getElementById('btn-trains')?.addEventListener('click', (e) => {
        view.showTrains = !view.showTrains;
        (e.target as HTMLElement).classList.toggle('active', view.showTrains);
    });
    document.getElementById('btn-signals')?.addEventListener('click', (e) => {
        view.showSignals = !view.showSignals;
        (e.target as HTMLElement).classList.toggle('active', view.showSignals);
    });
    document.getElementById('btn-stations')?.addEventListener('click', (e) => {
        view.showStations = !view.showStations;
        (e.target as HTMLElement).classList.toggle('active', view.showStations);
    });
}

function connectWebSocket() {
    const params = new URLSearchParams(window.location.search);
    if (params.get('demo') === '1') {
        console.log('Demo mode - no WebSocket');
        setStatus(true);
        startDemoMode();
        return;
    }

    console.log(`[WS] Connecting to ${HDDS_WS_URL}...`);

    ws = new WebSocket(HDDS_WS_URL);

    ws.onopen = () => {
        console.log('[WS] Connected to hdds-ws bridge');
        setStatus(true);

        // Subscribe to all DDS topics via hdds-ws protocol
        for (const topic of TOPICS) {
            ws!.send(JSON.stringify({ type: 'subscribe', topic }));
        }

        if (reconnectTimer) {
            clearTimeout(reconnectTimer);
            reconnectTimer = null;
        }
    };

    ws.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            handleWsMessage(msg);
        } catch (e) {
            console.error('[WS] Parse error:', e);
        }
    };

    ws.onclose = () => {
        console.log('[WS] Disconnected');
        setStatus(false);
        ws = null;
        reconnectTimer = window.setTimeout(connectWebSocket, 2000);
    };

    ws.onerror = (e) => {
        console.error('[WS] Error:', e);
    };
}

function setStatus(connected: boolean) {
    const el = document.getElementById('connection-status')!;
    el.textContent = connected ? 'ONLINE' : 'OFFLINE';
    el.className = connected ? 'connected' : 'disconnected';
}

/**
 * Handle hdds-ws protocol messages
 */
function handleWsMessage(msg: any) {
    switch (msg.type) {
        case 'welcome':
            console.log(`[WS] Connected to DDS domain ${msg.domain} (hdds-ws v${msg.version})`);
            break;
        case 'subscribed':
            console.log(`[WS] Subscribed to ${msg.topic}`);
            break;
        case 'data':
            handleDdsData(msg.topic, msg.sample);
            break;
        case 'error':
            console.error(`[WS] DDS error [${msg.code}]: ${msg.message}`);
            break;
    }
}

/**
 * Handle incoming DDS data by topic
 */
function handleDdsData(topic: string, data: unknown) {
    switch (topic) {
        case 'railway/track_segment': {
            const seg = data as TrackSegmentJson;
            segments.set(seg.segment_id, seg);
            break;
        }
        case 'railway/train': {
            const train = data as TrainJson;
            trains.set(train.train_id, train);
            updateTrainsList();
            break;
        }
        case 'railway/signal': {
            const sig = data as SignalJson;
            signals.set(sig.signal_id, sig);
            break;
        }
        case 'railway/station': {
            const st = data as StationJson;
            stations.set(st.station_id, st);
            updateStationsList();
            break;
        }
        case 'railway/network_state': {
            networkState = data as NetworkStateJson;
            updateNetworkInfo();
            break;
        }
    }
}

function updateNetworkInfo() {
    if (!networkState) return;
    document.getElementById('train-count')!.textContent = String(networkState.train_count);
    document.getElementById('trains-running')!.textContent = String(networkState.trains_running);
    document.getElementById('trains-at-station')!.textContent = String(networkState.trains_at_station);
    document.getElementById('signals-red')!.textContent = String(networkState.signals_red);
    document.getElementById('signals-green')!.textContent = String(networkState.signals_green);
}

function updateTrainsList() {
    const container = document.getElementById('train-items')!;
    container.innerHTML = '';
    trains.forEach((train) => {
        const div = document.createElement('div');
        div.className = 'train-item';
        div.style.borderLeftColor = `#${train.color.toString(16).padStart(6, '0')}`;
        div.innerHTML = `
            <div class="name">${train.name}</div>
            <div class="status">${TRAIN_STATES[train.state] || 'UNKNOWN'} - ${train.speed.toFixed(1)} km/h</div>
        `;
        container.appendChild(div);
    });
}

function updateStationsList() {
    const container = document.getElementById('station-items')!;
    container.innerHTML = '';
    stations.forEach((station) => {
        const div = document.createElement('div');
        div.className = 'station-item';
        div.innerHTML = `
            <div class="name">${station.name}</div>
            <div class="status">${station.platform_count} platform(s)</div>
        `;
        container.appendChild(div);
    });
}

// Rendering
function render() {
    ctx.fillStyle = '#0a1520';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // Draw grid
    drawGrid();

    // Transform to center the network
    ctx.save();
    const scale = Math.min(canvas.width / 1200, canvas.height / 900) * 0.85;
    const offsetX = (canvas.width - 1200 * scale) / 2;
    const offsetY = (canvas.height - 900 * scale) / 2;
    ctx.translate(offsetX, offsetY);
    ctx.scale(scale, scale);

    if (view.showTracks) drawTracks();
    if (view.showStations) drawStations();
    if (view.showSignals) drawSignals();
    if (view.showTrains) drawTrains();

    ctx.restore();

    requestAnimationFrame(render);
}

function drawGrid() {
    ctx.strokeStyle = 'rgba(64, 128, 160, 0.1)';
    ctx.lineWidth = 1;

    const gridSize = 50;
    for (let x = 0; x < canvas.width; x += gridSize) {
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, canvas.height);
        ctx.stroke();
    }
    for (let y = 0; y < canvas.height; y += gridSize) {
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(canvas.width, y);
        ctx.stroke();
    }
}

function drawTracks() {
    ctx.strokeStyle = '#4080a0';
    ctx.lineWidth = 8;
    ctx.lineCap = 'round';

    segments.forEach((seg) => {
        ctx.beginPath();
        ctx.moveTo(seg.x1, seg.y1);
        ctx.lineTo(seg.x2, seg.y2);
        ctx.stroke();

        // Rail effect
        ctx.strokeStyle = '#304050';
        ctx.lineWidth = 4;
        ctx.beginPath();
        ctx.moveTo(seg.x1, seg.y1);
        ctx.lineTo(seg.x2, seg.y2);
        ctx.stroke();

        ctx.strokeStyle = '#4080a0';
        ctx.lineWidth = 8;
    });
}

function drawStations() {
    stations.forEach((station) => {
        // Station platform
        ctx.fillStyle = '#8866ff';
        ctx.fillRect(station.x - 30, station.y - 10, 60, 20);

        // Station name
        ctx.fillStyle = '#ffffff';
        ctx.font = 'bold 14px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText(station.name, station.x, station.y - 20);
    });
}

function drawSignals() {
    signals.forEach((signal) => {
        const seg = segments.get(signal.segment_id);
        if (!seg) return;

        // Calculate signal position on track
        const t = signal.position;
        const x = seg.x1 + (seg.x2 - seg.x1) * t;
        const y = seg.y1 + (seg.y2 - seg.y1) * t;

        // Signal pole
        ctx.strokeStyle = '#555';
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.moveTo(x, y);
        ctx.lineTo(x, y - 25);
        ctx.stroke();

        // Signal light
        ctx.fillStyle = SIGNAL_COLORS[signal.state] || '#888';
        ctx.beginPath();
        ctx.arc(x, y - 30, 8, 0, Math.PI * 2);
        ctx.fill();

        // Glow effect
        const gradient = ctx.createRadialGradient(x, y - 30, 0, x, y - 30, 20);
        gradient.addColorStop(0, SIGNAL_COLORS[signal.state] + '44');
        gradient.addColorStop(1, 'transparent');
        ctx.fillStyle = gradient;
        ctx.beginPath();
        ctx.arc(x, y - 30, 20, 0, Math.PI * 2);
        ctx.fill();
    });
}

function drawTrains() {
    trains.forEach((train) => {
        const seg = segments.get(train.current_segment);
        if (!seg) return;

        // Calculate train position on track
        const t = train.position;
        const x = seg.x1 + (seg.x2 - seg.x1) * t;
        const y = seg.y1 + (seg.y2 - seg.y1) * t;

        // Train direction
        const dx = seg.x2 - seg.x1;
        const dy = seg.y2 - seg.y1;
        const angle = Math.atan2(dy, dx);

        ctx.save();
        ctx.translate(x, y);
        ctx.rotate(angle);

        // Train body
        const color = `#${train.color.toString(16).padStart(6, '0')}`;
        ctx.fillStyle = color;
        ctx.fillRect(-20, -8, 40, 16);

        // Train front
        ctx.beginPath();
        ctx.moveTo(20, -8);
        ctx.lineTo(28, 0);
        ctx.lineTo(20, 8);
        ctx.closePath();
        ctx.fill();

        // Windows
        ctx.fillStyle = 'rgba(255, 255, 255, 0.3)';
        ctx.fillRect(-15, -6, 8, 12);
        ctx.fillRect(-3, -6, 8, 12);
        ctx.fillRect(9, -6, 8, 12);

        ctx.restore();

        // Train name label
        ctx.fillStyle = '#ffffff';
        ctx.font = '12px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText(train.name, x, y - 20);
    });
}

// Demo mode - generate fake data
function startDemoMode() {
    // Add demo track segments (oval)
    const demoSegments: TrackSegmentJson[] = [
        { segment_id: 1, track_type: 0, x1: 200, y1: 200, x2: 500, y2: 200, next_segment: 2, prev_segment: 8, length: 300, speed_limit: 100, has_platform: true },
        { segment_id: 2, track_type: 0, x1: 500, y1: 200, x2: 800, y2: 200, next_segment: 3, prev_segment: 1, length: 300, speed_limit: 100, has_platform: false },
        { segment_id: 3, track_type: 1, x1: 800, y1: 200, x2: 1000, y2: 400, next_segment: 4, prev_segment: 2, length: 283, speed_limit: 80, has_platform: false },
        { segment_id: 4, track_type: 0, x1: 1000, y1: 400, x2: 1000, y2: 600, next_segment: 5, prev_segment: 3, length: 200, speed_limit: 100, has_platform: true },
        { segment_id: 5, track_type: 1, x1: 1000, y1: 600, x2: 800, y2: 700, next_segment: 6, prev_segment: 4, length: 224, speed_limit: 80, has_platform: false },
        { segment_id: 6, track_type: 0, x1: 800, y1: 700, x2: 400, y2: 700, next_segment: 7, prev_segment: 5, length: 400, speed_limit: 100, has_platform: true },
        { segment_id: 7, track_type: 1, x1: 400, y1: 700, x2: 200, y2: 500, next_segment: 8, prev_segment: 6, length: 283, speed_limit: 80, has_platform: false },
        { segment_id: 8, track_type: 0, x1: 200, y1: 500, x2: 200, y2: 200, next_segment: 1, prev_segment: 7, length: 300, speed_limit: 100, has_platform: true },
    ];

    demoSegments.forEach(seg => segments.set(seg.segment_id, seg));

    // Demo stations
    const demoStations: StationJson[] = [
        { station_id: 1, name: 'Central', segment_id: 1, position: 0.5, x: 350, y: 160, platform_count: 2 },
        { station_id: 2, name: 'East Side', segment_id: 4, position: 0.5, x: 1040, y: 500, platform_count: 1 },
        { station_id: 3, name: 'South', segment_id: 6, position: 0.5, x: 600, y: 740, platform_count: 2 },
        { station_id: 4, name: 'West End', segment_id: 8, position: 0.5, x: 160, y: 350, platform_count: 1 },
    ];
    demoStations.forEach(st => stations.set(st.station_id, st));
    updateStationsList();

    // Demo signals
    const demoSignals: SignalJson[] = [
        { signal_id: 1, segment_id: 1, position: 0.8, state: 2, timestamp_us: 0 },
        { signal_id: 2, segment_id: 3, position: 0.5, state: 0, timestamp_us: 0 },
        { signal_id: 3, segment_id: 5, position: 0.5, state: 2, timestamp_us: 0 },
        { signal_id: 4, segment_id: 7, position: 0.5, state: 1, timestamp_us: 0 },
    ];
    demoSignals.forEach(sig => signals.set(sig.signal_id, sig));

    // Demo trains
    const demoTrains: TrainJson[] = [
        { train_id: 1, name: 'Express A', color: 0xFF4444, current_segment: 1, position: 0.3, speed: 80, direction: 1, state: 2, next_station: 0, delay_seconds: 0, timestamp_us: 0 },
        { train_id: 2, name: 'Local B', color: 0x44FF44, current_segment: 4, position: 0.5, speed: 0, direction: 1, state: 4, next_station: 2, delay_seconds: 0, timestamp_us: 0 },
        { train_id: 3, name: 'Night C', color: 0x4444FF, current_segment: 6, position: 0.7, speed: 60, direction: 1, state: 2, next_station: 0, delay_seconds: 0, timestamp_us: 0 },
    ];
    demoTrains.forEach(train => trains.set(train.train_id, train));
    updateTrainsList();

    // Network state
    networkState = {
        train_count: 3,
        trains_running: 2,
        trains_at_station: 1,
        signals_red: 1,
        signals_green: 2,
        avg_delay_seconds: 0,
        timestamp_us: 0,
    };
    updateNetworkInfo();

    // Animate demo trains
    setInterval(() => {
        demoTrains.forEach(train => {
            if (train.state === 2 || train.state === 1 || train.state === 3) { // moving states
                train.position += 0.005 * train.direction;
                if (train.position >= 1) {
                    train.position = 0;
                    train.current_segment = (train.current_segment % 8) + 1;
                }
                trains.set(train.train_id, { ...train });
            }
        });

        // Cycle signals randomly
        if (Math.random() < 0.02) {
            const sigId = Math.floor(Math.random() * 4) + 1;
            const sig = signals.get(sigId);
            if (sig) {
                sig.state = (sig.state + 1) % 3;
                signals.set(sigId, { ...sig });
            }
        }
    }, 50);
}

// Start
document.addEventListener('DOMContentLoaded', init);
