/**
 * Air Defense Radar Viewer - Main Entry Point
 *
 * Connects to HDDS via hdds-ws bridge (WebSocket JSON protocol).
 */

import { RadarScope } from './radar.js';
import { TrackManager } from './tracks.js';
import { EffectsManager } from './effects.js';

// Configuration
const HDDS_WS_URL = `ws://${window.location.hostname || 'localhost'}:9090/ws`;
const DEMO_MODE = new URLSearchParams(window.location.search).has('demo');
const TOPICS = ['RadarSweep', 'AircraftTrack', 'SamSite', 'Engagement', 'EngagementResult', 'RadarAlert'];

// Global state
let radarScope = null;
let trackManager = null;
let effectsManager = null;
let ws = null;
let connected = false;

// DOM elements
const connectionBar = document.getElementById('connection-bar');
const statusDot = document.querySelector('.status-dot');
const statusText = document.querySelector('.status-text');
const trackCountEl = document.getElementById('track-count');
const hostileCountEl = document.getElementById('hostile-count');
const friendlyCountEl = document.getElementById('friendly-count');
const engagementCountEl = document.getElementById('engagement-count');
const sweepAngleEl = document.getElementById('sweep-angle');
const samSitesList = document.getElementById('sam-sites-list');
const alertsList = document.getElementById('alerts-list');

/**
 * Initialize the radar viewer
 */
function init() {
    console.log('[RADAR] Initializing Air Defense Radar Viewer');
    console.log('[RADAR] Demo mode:', DEMO_MODE);

    // Create canvas
    const canvas = document.getElementById('radar-canvas');
    const size = Math.min(window.innerWidth, window.innerHeight) - 80;
    canvas.width = size;
    canvas.height = size;

    // Initialize components
    radarScope = new RadarScope(canvas);
    trackManager = new TrackManager();
    effectsManager = new EffectsManager();

    // Connect to WebSocket (or start demo mode)
    if (DEMO_MODE) {
        console.log('[RADAR] Starting demo mode');
        startDemoMode();
    } else {
        connectWebSocket();
    }

    // Start render loop
    requestAnimationFrame(render);

    // Handle window resize
    window.addEventListener('resize', () => {
        const size = Math.min(window.innerWidth, window.innerHeight) - 80;
        canvas.width = size;
        canvas.height = size;
        radarScope.resize(size);
    });
}

/**
 * Connect to WebSocket server
 */
function connectWebSocket() {
    console.log(`[WS] Connecting to ${HDDS_WS_URL}...`);

    ws = new WebSocket(HDDS_WS_URL);

    ws.onopen = () => {
        console.log('[WS] Connected');
        connected = true;
        updateConnectionUI(true);

        // Subscribe to all DDS topics via hdds-ws protocol
        for (const topic of TOPICS) {
            ws.send(JSON.stringify({ type: 'subscribe', topic }));
        }
    };

    ws.onclose = () => {
        console.log('[WS] Disconnected');
        connected = false;
        updateConnectionUI(false);

        // Reconnect after delay
        setTimeout(connectWebSocket, 2000);
    };

    ws.onerror = (error) => {
        console.error('[WS] Error:', error);
    };

    ws.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            handleWsMessage(msg);
        } catch (e) {
            console.warn('[WS] Failed to parse message:', e);
        }
    };
}

/**
 * Handle hdds-ws protocol messages
 */
function handleWsMessage(msg) {
    switch (msg.type) {
        case 'welcome':
            console.log(`[WS] Connected to DDS domain ${msg.domain} (hdds-ws v${msg.version})`);
            break;
        case 'subscribed':
            console.log(`[WS] Subscribed to ${msg.topic}`);
            break;
        case 'data':
            handleMessage({ topic: msg.topic, data: msg.sample });
            break;
        case 'error':
            console.error(`[WS] DDS error [${msg.code}]: ${msg.message}`);
            break;
    }
}

/**
 * Handle incoming DDS data message
 */
function handleMessage(msg) {
    const { topic, data } = msg;

    switch (topic) {
        case 'RadarSweep':
            radarScope.setSweepAngle(data.azimuth_deg);
            sweepAngleEl.textContent = `${Math.round(data.azimuth_deg).toString().padStart(3, '0')}°`;
            break;

        case 'AircraftTrack':
            trackManager.updateTrack(data);
            updateStats();
            break;

        case 'SamSite':
            updateSamSite(data);
            break;

        case 'Engagement':
            effectsManager.updateEngagement(data);
            updateStats();
            break;

        case 'EngagementResult':
            if (data.success === 1) {
                effectsManager.createExplosion(
                    trackManager.getTrackPosition(data.track_id)
                );
                trackManager.removeTrack(data.track_id);
            }
            effectsManager.removeEngagement(data.engagement_id);
            addAlert(data.success === 1 ? 'TARGET DESTROYED' : 'MISSILE MISS',
                     data.success === 1 ? 'critical' : 'warning');
            break;

        case 'RadarAlert':
            addAlert(data.message, getPriorityClass(data.priority));
            break;
    }
}

/**
 * Update connection UI
 */
function updateConnectionUI(isConnected) {
    connectionBar.classList.toggle('connected', isConnected);
    statusDot.classList.toggle('online', isConnected);
    statusDot.classList.toggle('offline', !isConnected);
    statusText.textContent = isConnected ? 'Connected' : 'Disconnected';
}

/**
 * Update stats display
 */
function updateStats() {
    const tracks = trackManager.getTracks();
    const hostileCount = tracks.filter(t => t.status === 'Hostile').length;
    const friendlyCount = tracks.filter(t => t.status === 'Friendly').length;
    const engagementCount = effectsManager.getEngagementCount();

    trackCountEl.textContent = tracks.length;
    hostileCountEl.textContent = hostileCount;
    friendlyCountEl.textContent = friendlyCount;
    engagementCountEl.textContent = engagementCount;
}

/**
 * Update SAM site display
 */
function updateSamSite(site) {
    let el = document.getElementById(`sam-${site.site_id}`);
    if (!el) {
        el = document.createElement('div');
        el.id = `sam-${site.site_id}`;
        el.className = 'sam-site';
        samSitesList.appendChild(el);
    }

    el.innerHTML = `
        <span class="site-status ${site.active ? '' : 'inactive'}"></span>
        <span class="site-name">${site.site_name}</span>
        <span class="missile-count">${site.missiles_ready}/${site.missiles_total}</span>
    `;

    // Also update radar scope's SAM site data
    radarScope.updateSamSite(site);
}

/**
 * Add alert to the alerts list
 */
function addAlert(message, priority = 'info') {
    const time = new Date().toLocaleTimeString('en-US', { hour12: false });

    const el = document.createElement('div');
    el.className = `alert ${priority}`;
    el.innerHTML = `<span class="alert-time">${time}</span>${message}`;

    alertsList.insertBefore(el, alertsList.firstChild);

    // Keep only last 10 alerts
    while (alertsList.children.length > 10) {
        alertsList.removeChild(alertsList.lastChild);
    }
}

function getPriorityClass(priority) {
    switch (priority) {
        case 2: return 'critical';
        case 1: return 'warning';
        default: return 'info';
    }
}

/**
 * Main render loop
 */
function render() {
    // Clear and draw radar background
    radarScope.clear();
    radarScope.drawGrid();
    radarScope.drawSamCoverage();

    // Draw tracks
    const tracks = trackManager.getTracks();
    for (const track of tracks) {
        radarScope.drawTrack(track);
    }

    // Draw engagements
    const engagements = effectsManager.getEngagements();
    for (const engagement of engagements) {
        radarScope.drawEngagement(engagement);
    }

    // Draw explosions
    effectsManager.updateExplosions();
    const explosions = effectsManager.getExplosions();
    for (const explosion of explosions) {
        radarScope.drawExplosion(explosion);
    }

    // Draw sweep line (on top)
    radarScope.drawSweep();

    requestAnimationFrame(render);
}

/**
 * Demo mode - simulate data without backend
 */
function startDemoMode() {
    updateConnectionUI(true);
    statusText.textContent = 'Demo Mode';

    let sweepAngle = 0;
    let trackId = 1;
    const demoTracks = [];

    // Generate initial tracks
    for (let i = 0; i < 8; i++) {
        demoTracks.push(createDemoTrack(trackId++));
    }

    // Demo SAM sites
    const samSites = [
        { site_id: 1, x_km: -50, y_km: 30, range_km: 80, missiles_ready: 8, missiles_total: 8, site_name: 'PATRIOT-1', active: true },
        { site_id: 2, x_km: 40, y_km: 40, range_km: 80, missiles_ready: 6, missiles_total: 8, site_name: 'PATRIOT-2', active: true },
        { site_id: 3, x_km: 0, y_km: -60, range_km: 120, missiles_ready: 4, missiles_total: 8, site_name: 'S-300', active: true },
    ];

    samSites.forEach(site => updateSamSite(site));

    // Simulation loop
    setInterval(() => {
        // Update sweep
        sweepAngle = (sweepAngle + 9) % 360; // 90°/s at 10Hz
        handleMessage({ topic: 'RadarSweep', data: { azimuth_deg: sweepAngle } });

        // Update tracks
        for (const track of demoTracks) {
            updateDemoTrack(track);
            handleMessage({ topic: 'AircraftTrack', data: track });
        }

        // Spawn new tracks occasionally
        if (Math.random() < 0.02 && demoTracks.length < 12) {
            const newTrack = createDemoTrack(trackId++);
            demoTracks.push(newTrack);
            if (newTrack.status === 'Hostile') {
                addAlert(`New hostile contact: ${newTrack.callsign}`, 'warning');
            }
        }

        // Remove tracks that left the area
        for (let i = demoTracks.length - 1; i >= 0; i--) {
            const dist = Math.sqrt(demoTracks[i].x_km ** 2 + demoTracks[i].y_km ** 2);
            if (dist > 220) {
                demoTracks.splice(i, 1);
            }
        }
    }, 100);
}

function createDemoTrack(id) {
    const angle = Math.random() * Math.PI * 2;
    const dist = 180;
    const statuses = ['Friendly', 'Hostile', 'Hostile', 'Neutral'];
    const status = statuses[Math.floor(Math.random() * statuses.length)];
    const prefix = status === 'Hostile' ? 'HO' : status === 'Friendly' ? 'FR' : 'NT';

    return {
        track_id: id,
        x_km: Math.sin(angle) * dist,
        y_km: Math.cos(angle) * dist,
        altitude_m: 5000 + Math.random() * 7000,
        heading_deg: (angle * 180 / Math.PI + 180 + (Math.random() - 0.5) * 60) % 360,
        speed_kts: 300 + Math.random() * 300,
        status: status,
        callsign: `${prefix}-${1000 + Math.floor(Math.random() * 9000)}`,
        rcs: Math.random() * 30 - 10,
    };
}

function updateDemoTrack(track) {
    const speedKms = track.speed_kts * 0.000514444;
    const headingRad = track.heading_deg * Math.PI / 180;
    track.x_km += Math.sin(headingRad) * speedKms * 0.1;
    track.y_km += Math.cos(headingRad) * speedKms * 0.1;
    track.heading_deg += (Math.random() - 0.5) * 2;
}

// Initialize on DOM ready
document.addEventListener('DOMContentLoaded', init);
