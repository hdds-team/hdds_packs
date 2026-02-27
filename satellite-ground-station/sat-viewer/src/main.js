/**
 * HDDS Satellite Ground Station - Canvas 2D Viewer
 *
 * Connects to hdds-ws WebSocket bridge on port 9090 and renders
 * a world map with satellite positions, ground stations, tracking
 * links, telemetry data, and orbital alerts.
 */

// =============================================================================
// CONFIGURATION
// =============================================================================

const hostname = window.location.hostname || 'localhost';
const HDDS_WS_URL = `ws://${hostname}:9090/ws`;
const RECONNECT_DELAY = 3000;

const TOPICS = {
    SAT_POSITION:   'sat/position',
    GROUND_STATION: 'sat/station',
    PASS_PREDICTION:'sat/pass',
    TELEMETRY:      'sat/telemetry',
    ORBITAL_ALERT:  'sat/alert',
};

// Colors
const COLORS = {
    background:     '#0a0a1a',
    gridLines:      '#1a1a3a',
    gridLabels:     '#2a2a4a',
    coastline:      '#1a2a1a',
    landFill:       '#0d1a0d',
    ocean:          '#0a0e1a',
    stationIcon:    '#ffa500',
    stationCircle:  'rgba(255, 165, 0, 0.08)',
    trackingLink:   'rgba(255, 165, 0, 0.6)',
    trailDefault:   'rgba(100, 100, 200, 0.3)',
};

// Satellite color by name prefix
const SAT_COLORS = {
    'ISS':       '#ffffff',
    'Hubble':    '#c060ff',
    'GPS':       '#40e040',
    'Starlink':  '#00e0e0',
    'GOES':      '#ffe040',
};

function satColor(name) {
    for (const [prefix, color] of Object.entries(SAT_COLORS)) {
        if (name.startsWith(prefix)) return color;
    }
    return '#8080ff';
}

// Orbit trail length
const TRAIL_LENGTH = 120;

// =============================================================================
// STATE
// =============================================================================

const state = {
    satellites: new Map(),   // norad_id -> { ...position, trail: [{lat, lon}] }
    stations: new Map(),     // station_id -> { ...station data }
    telemetry: new Map(),    // norad_id -> { ...latest telemetry }
    passes: [],              // recent pass events (max 30)
    alerts: [],              // recent alerts (max 20)
    connected: false,
};

// =============================================================================
// CANVAS SETUP
// =============================================================================

const canvas = document.getElementById('main-canvas');
const ctx = canvas.getContext('2d');

function resizeCanvas() {
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
}

window.addEventListener('resize', resizeCanvas);
resizeCanvas();

// Equirectangular projection helpers
function lonToX(lon) {
    return (lon + 180) / 360 * canvas.width;
}

function latToY(lat) {
    return (90 - lat) / 180 * canvas.height;
}

// =============================================================================
// SIMPLIFIED CONTINENT OUTLINES (very rough polygons)
// =============================================================================

const CONTINENTS = [
    // North America (simplified)
    [
        [-130, 50], [-125, 55], [-120, 60], [-140, 62], [-165, 64], [-168, 66],
        [-165, 70], [-130, 72], [-85, 72], [-65, 62], [-55, 48], [-65, 44],
        [-70, 42], [-75, 35], [-80, 30], [-82, 25], [-90, 20], [-98, 18],
        [-105, 20], [-110, 25], [-115, 30], [-120, 35], [-125, 42], [-130, 50],
    ],
    // South America
    [
        [-80, 10], [-75, 12], [-62, 10], [-55, 5], [-50, 0], [-48, -5],
        [-45, -10], [-38, -12], [-35, -20], [-40, -25], [-48, -28],
        [-55, -33], [-58, -38], [-65, -42], [-68, -50], [-72, -55],
        [-75, -50], [-72, -42], [-70, -35], [-72, -20], [-77, -10],
        [-80, 0], [-80, 10],
    ],
    // Europe
    [
        [-10, 36], [-5, 36], [0, 38], [3, 42], [5, 44], [3, 48],
        [5, 50], [8, 54], [10, 56], [12, 58], [18, 60], [25, 62],
        [28, 66], [30, 70], [20, 70], [10, 65], [5, 60], [0, 55],
        [-5, 50], [-10, 45], [-10, 36],
    ],
    // Africa
    [
        [-15, 30], [-5, 36], [10, 37], [12, 34], [20, 32], [32, 32],
        [35, 30], [40, 12], [50, 12], [50, 5], [45, 0], [42, -5],
        [40, -12], [35, -22], [30, -28], [28, -33], [20, -35],
        [15, -30], [12, -20], [10, -10], [8, 0], [5, 5], [0, 5],
        [-5, 5], [-10, 5], [-15, 10], [-18, 15], [-15, 20], [-15, 30],
    ],
    // Asia (simplified)
    [
        [30, 70], [40, 68], [50, 65], [60, 60], [70, 55], [75, 55],
        [80, 50], [85, 48], [90, 28], [95, 20], [100, 15], [105, 10],
        [108, 20], [115, 22], [120, 30], [125, 35], [130, 40],
        [135, 35], [140, 45], [145, 50], [155, 55], [165, 60],
        [180, 65], [170, 68], [150, 70], [120, 72], [80, 72],
        [60, 72], [40, 72], [30, 70],
    ],
    // Australia
    [
        [115, -15], [130, -12], [138, -12], [142, -15], [145, -18],
        [150, -24], [153, -28], [150, -35], [145, -38], [137, -35],
        [130, -33], [122, -33], [115, -30], [114, -25], [115, -15],
    ],
];

// =============================================================================
// DRAWING FUNCTIONS
// =============================================================================

function drawBackground() {
    ctx.fillStyle = COLORS.ocean;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
}

function drawContinents() {
    ctx.fillStyle = COLORS.landFill;
    ctx.strokeStyle = COLORS.coastline;
    ctx.lineWidth = 1;

    for (const poly of CONTINENTS) {
        ctx.beginPath();
        for (let i = 0; i < poly.length; i++) {
            const x = lonToX(poly[i][0]);
            const y = latToY(poly[i][1]);
            if (i === 0) ctx.moveTo(x, y);
            else ctx.lineTo(x, y);
        }
        ctx.closePath();
        ctx.fill();
        ctx.stroke();
    }
}

function drawGrid() {
    ctx.strokeStyle = COLORS.gridLines;
    ctx.lineWidth = 0.5;
    ctx.font = '10px monospace';
    ctx.fillStyle = COLORS.gridLabels;

    // Latitude lines every 30 degrees
    for (let lat = -60; lat <= 60; lat += 30) {
        const y = latToY(lat);
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(canvas.width, y);
        ctx.stroke();
        ctx.fillText(`${lat >= 0 ? lat + 'N' : Math.abs(lat) + 'S'}`, 4, y - 3);
    }

    // Longitude lines every 30 degrees
    for (let lon = -180; lon <= 180; lon += 30) {
        const x = lonToX(lon);
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, canvas.height);
        ctx.stroke();
        if (lon !== -180 && lon !== 180) {
            ctx.fillText(`${lon >= 0 ? lon + 'E' : Math.abs(lon) + 'W'}`, x + 3, canvas.height - 5);
        }
    }

    // Equator and prime meridian slightly brighter
    ctx.strokeStyle = '#252540';
    ctx.lineWidth = 1;

    // Equator
    const eqY = latToY(0);
    ctx.beginPath();
    ctx.moveTo(0, eqY);
    ctx.lineTo(canvas.width, eqY);
    ctx.stroke();

    // Prime meridian
    const pmX = lonToX(0);
    ctx.beginPath();
    ctx.moveTo(pmX, 0);
    ctx.lineTo(pmX, canvas.height);
    ctx.stroke();
}

function drawGroundStations() {
    for (const [, gs] of state.stations) {
        const x = lonToX(gs.lon_deg);
        const y = latToY(gs.lat_deg);

        // Visibility circle (approximate, ~2000km radius for LEO)
        ctx.beginPath();
        // Rough: 2000km ~ 18 degrees latitude
        const radiusDeg = 18;
        const rx = (radiusDeg / 360) * canvas.width;
        const ry = (radiusDeg / 180) * canvas.height;
        ctx.ellipse(x, y, rx, ry, 0, 0, 2 * Math.PI);
        ctx.fillStyle = COLORS.stationCircle;
        ctx.fill();

        // Antenna icon: triangle pointing up
        const sz = 8;
        ctx.beginPath();
        ctx.moveTo(x, y - sz);
        ctx.lineTo(x - sz * 0.7, y + sz * 0.5);
        ctx.lineTo(x + sz * 0.7, y + sz * 0.5);
        ctx.closePath();
        ctx.fillStyle = COLORS.stationIcon;
        ctx.fill();

        // Station label
        ctx.font = '10px monospace';
        ctx.fillStyle = '#cc8800';
        ctx.textAlign = 'center';
        ctx.fillText(gs.station_id || gs.name, x, y + sz + 12);

        // Mode indicator
        if (gs.mode === 1 || gs.mode === 2) { // TRACKING or DOWNLINK
            ctx.fillStyle = '#4ade80';
            ctx.fillText(gs.tracking_sat || 'tracking', x, y + sz + 24);
        }

        ctx.textAlign = 'left';
    }
}

function drawTrackingLinks() {
    for (const [, gs] of state.stations) {
        if (!gs.tracking_sat || gs.tracking_sat === '') continue;

        // Find the satellite being tracked
        let satData = null;
        for (const [, sat] of state.satellites) {
            if (sat.name === gs.tracking_sat) {
                satData = sat;
                break;
            }
        }
        if (!satData) continue;

        const sx = lonToX(gs.lon_deg);
        const sy = latToY(gs.lat_deg);
        const ex = lonToX(satData.lon_deg);
        const ey = latToY(satData.lat_deg);

        // Dashed line from station to satellite
        ctx.setLineDash([4, 4]);
        ctx.strokeStyle = COLORS.trackingLink;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(sx, sy);
        ctx.lineTo(ex, ey);
        ctx.stroke();
        ctx.setLineDash([]);
    }
}

function drawSatellites() {
    for (const [, sat] of state.satellites) {
        const x = lonToX(sat.lon_deg);
        const y = latToY(sat.lat_deg);
        const color = satColor(sat.name);

        // Orbit trail
        if (sat.trail && sat.trail.length > 1) {
            ctx.lineWidth = 1;
            for (let i = 1; i < sat.trail.length; i++) {
                const alpha = i / sat.trail.length * 0.6;
                ctx.strokeStyle = color.replace(')', `, ${alpha})`).replace('rgb', 'rgba').replace('#', '');
                // For hex colors, convert to rgba
                const r = parseInt(color.slice(1, 3), 16) || 128;
                const g = parseInt(color.slice(3, 5), 16) || 128;
                const b = parseInt(color.slice(5, 7), 16) || 128;
                ctx.strokeStyle = `rgba(${r}, ${g}, ${b}, ${alpha})`;

                const px = lonToX(sat.trail[i - 1].lon);
                const py = latToY(sat.trail[i - 1].lat);
                const cx = lonToX(sat.trail[i].lon);
                const cy = latToY(sat.trail[i].lat);

                // Skip if the trail wraps around the map edge
                if (Math.abs(cx - px) > canvas.width * 0.5) continue;

                ctx.beginPath();
                ctx.moveTo(px, py);
                ctx.lineTo(cx, cy);
                ctx.stroke();
            }
        }

        // Satellite dot
        const radius = sat.alt_km > 10000 ? 5 : sat.alt_km > 1000 ? 4 : 3;
        ctx.beginPath();
        ctx.arc(x, y, radius, 0, 2 * Math.PI);
        ctx.fillStyle = color;
        ctx.fill();

        // Glow
        ctx.beginPath();
        ctx.arc(x, y, radius + 3, 0, 2 * Math.PI);
        const r = parseInt(color.slice(1, 3), 16) || 128;
        const g = parseInt(color.slice(3, 5), 16) || 128;
        const b = parseInt(color.slice(5, 7), 16) || 128;
        ctx.fillStyle = `rgba(${r}, ${g}, ${b}, 0.2)`;
        ctx.fill();

        // Label
        ctx.font = '10px monospace';
        ctx.fillStyle = color;
        ctx.textAlign = 'center';
        ctx.fillText(sat.name, x, y - radius - 5);

        // Status indicator
        if (sat.status === 2) { // ECLIPSE
            ctx.fillStyle = '#666';
            ctx.fillText('[ECL]', x, y - radius - 16);
        } else if (sat.status === 3) { // SAFEMODE
            ctx.fillStyle = '#f87171';
            ctx.fillText('[SAFE]', x, y - radius - 16);
        }

        ctx.textAlign = 'left';
    }
}

// =============================================================================
// HUD UPDATES
// =============================================================================

function updateTelemetryPanel() {
    const container = document.getElementById('telem-content');
    if (state.telemetry.size === 0) {
        container.innerHTML = '<div style="color:#4a4a6a;font-size:11px;">Waiting for telemetry...</div>';
        return;
    }

    let html = '';
    for (const [noradId, t] of state.telemetry) {
        const sat = state.satellites.get(noradId);
        const name = sat ? sat.name : noradId;
        const color = satColor(name);
        html += `
            <div class="telem-block" style="border-left-color:${color}">
                <div class="sat-name" style="color:${color}">${name}</div>
                <div class="telem-row"><span>Station</span><span class="val">${t.station_id}</span></div>
                <div class="telem-row"><span>Battery</span><span class="val">${t.battery_v.toFixed(1)} V</span></div>
                <div class="telem-row"><span>Solar</span><span class="val">${t.solar_w.toFixed(0)} W</span></div>
                <div class="telem-row"><span>CPU Temp</span><span class="val">${t.cpu_temp_c.toFixed(1)} C</span></div>
                <div class="telem-row"><span>Signal</span><span class="val">${t.signal_strength_db.toFixed(1)} dB</span></div>
                <div class="telem-row"><span>Frame #</span><span class="val">${t.frame_seq}</span></div>
                <div class="telem-row"><span>Errors</span><span class="val">${t.bit_errors}</span></div>
            </div>
        `;
    }
    container.innerHTML = html;
}

function updatePassLog() {
    const container = document.getElementById('pass-list');
    if (state.passes.length === 0) {
        container.innerHTML = '<div style="color:#4a4a6a;font-size:11px;">No pass events yet</div>';
        return;
    }

    let html = '';
    // Show most recent first
    for (let i = state.passes.length - 1; i >= Math.max(0, state.passes.length - 15); i--) {
        const p = state.passes[i];
        const phaseClass = p.phase === 1 ? 'phase-aos' : p.phase === 3 ? 'phase-los' : 'phase-track';
        const phaseName = p.phase === 1 ? 'AOS' : p.phase === 3 ? 'LOS' : 'TRACK';
        html += `<div class="pass-item"><span class="${phaseClass}">[${phaseName}]</span> ${p.station_id} - ${p.norad_id} el:${(p.max_elevation_deg || 0).toFixed(1)}</div>`;
    }
    container.innerHTML = html;
}

function updateAlertLog() {
    const container = document.getElementById('alert-list');
    if (state.alerts.length === 0) {
        container.innerHTML = '<div style="color:#4a4a6a;font-size:11px;">No alerts</div>';
        return;
    }

    let html = '';
    for (let i = state.alerts.length - 1; i >= Math.max(0, state.alerts.length - 10); i--) {
        const a = state.alerts[i];
        const isNew = i === state.alerts.length - 1 ? 'alert-new' : '';
        html += `<div class="alert-item ${isNew}">[${a.alert_type}] ${a.description}</div>`;
    }
    container.innerHTML = html;
}

// =============================================================================
// WEBSOCKET CONNECTION
// =============================================================================

let ws = null;

function connect() {
    ws = new WebSocket(HDDS_WS_URL);

    ws.onopen = () => {
        state.connected = true;
        document.getElementById('connection-text').textContent = 'Connected to HDDS';
        document.getElementById('connection-text').className = 'conn-on';

        // Subscribe to all satellite topics
        for (const topic of Object.values(TOPICS)) {
            ws.send(JSON.stringify({ type: 'subscribe', topic }));
        }

        // Hide loading screen
        document.getElementById('loading').classList.add('hidden');
    };

    ws.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            handleMessage(msg);
        } catch (e) {
            // ignore parse errors
        }
    };

    ws.onclose = () => {
        state.connected = false;
        document.getElementById('connection-text').textContent = 'Disconnected - reconnecting...';
        document.getElementById('connection-text').className = 'conn-off';
        setTimeout(connect, RECONNECT_DELAY);
    };

    ws.onerror = () => {
        ws.close();
    };
}

function handleMessage(msg) {
    if (msg.type !== 'data') return;
    const topic = msg.topic;
    const data = msg.sample;

    if (topic === TOPICS.SAT_POSITION) {
        const existing = state.satellites.get(data.norad_id);
        const trail = existing ? existing.trail : [];

        // Add to trail
        trail.push({ lat: data.lat_deg, lon: data.lon_deg });
        if (trail.length > TRAIL_LENGTH) {
            trail.shift();
        }

        state.satellites.set(data.norad_id, { ...data, trail });
    }
    else if (topic === TOPICS.GROUND_STATION) {
        state.stations.set(data.station_id, data);
    }
    else if (topic === TOPICS.PASS_PREDICTION) {
        state.passes.push(data);
        if (state.passes.length > 50) state.passes.shift();
        updatePassLog();
    }
    else if (topic === TOPICS.TELEMETRY) {
        state.telemetry.set(data.norad_id, data);
        updateTelemetryPanel();
    }
    else if (topic === TOPICS.ORBITAL_ALERT) {
        state.alerts.push(data);
        if (state.alerts.length > 30) state.alerts.shift();
        updateAlertLog();
    }
}

// =============================================================================
// RENDER LOOP
// =============================================================================

function render() {
    drawBackground();
    drawContinents();
    drawGrid();
    drawGroundStations();
    drawTrackingLinks();
    drawSatellites();

    // Draw satellite count in top area
    ctx.font = '11px monospace';
    ctx.fillStyle = '#4a4a6a';
    ctx.textAlign = 'left';
    const satCount = state.satellites.size;
    const stCount = state.stations.size;
    const passCount = state.passes.length;
    ctx.fillText(
        `SAT: ${satCount}  GS: ${stCount}  PASSES: ${passCount}`,
        12,
        canvas.height - 8
    );

    requestAnimationFrame(render);
}

// =============================================================================
// INIT
// =============================================================================

connect();
requestAnimationFrame(render);
