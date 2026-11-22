/**
 * Predictive Maintenance Dashboard - Main Entry Point
 *
 * Connects to HDDS via hdds-ws bridge (WebSocket JSON protocol).
 */

// Configuration
const HDDS_WS_URL = `ws://${window.location.hostname || 'localhost'}:9090/ws`;
const DEMO_MODE = new URLSearchParams(window.location.search).has('demo');
// Topic names, mirroring the `topics` module of sensor-types: the viewer
// subscribes by DDS topic name, not by type name.
const TOPIC = {
    SENSOR_READING: 'rt/maintenance/SensorReading',
    MACHINE_STATUS: 'rt/maintenance/MachineStatus',
    ALERT: 'rt/maintenance/Alert',
    PREDICTION: 'rt/maintenance/MaintenancePrediction',
};
const TOPICS = Object.values(TOPIC);

// Sensor type mappings
const SENSOR_TYPES = {
    0: { name: 'Temperature', unit: '°C', color: '#f85149' },
    1: { name: 'Vibration', unit: 'mm/s', color: '#d29922' },
    2: { name: 'Pressure', unit: 'bar', color: '#58a6ff' },
    3: { name: 'Current', unit: 'A', color: '#a371f7' },
    4: { name: 'RPM', unit: 'rpm', color: '#3fb950' },
    5: { name: 'Flow Rate', unit: 'L/min', color: '#79c0ff' },
};

const MACHINE_STATUS = {
    0: 'stopped',
    1: 'running',
    2: 'warning',
    3: 'critical',
    4: 'maintenance',
};

// State
let ws = null;
let connected = false;
const machines = new Map();
const sensors = new Map();
const alerts = [];
const predictions = new Map();
let selectedMachineId = null;
let alertCount = 0;

// DOM Elements
const statusDot = document.querySelector('.status-dot');
const statusText = document.querySelector('.status-text');
const currentTime = document.getElementById('current-time');
const machinesList = document.getElementById('machines-list');
const sensorsGrid = document.getElementById('sensors-grid');
const alertsList = document.getElementById('alerts-list');
const predictionsList = document.getElementById('predictions-list');

// Stats elements
const totalMachinesEl = document.getElementById('total-machines');
const activeSensorsEl = document.getElementById('active-sensors');
const alertCountEl = document.getElementById('alert-count');
const avgHealthEl = document.getElementById('avg-health');

/**
 * Initialize the dashboard
 */
function init() {
    console.log('[PM] Initializing Predictive Maintenance Dashboard');
    console.log('[PM] Demo mode:', DEMO_MODE);

    // Start clock
    updateClock();
    setInterval(updateClock, 1000);

    if (DEMO_MODE) {
        startDemoMode();
    } else {
        connectWebSocket();
    }
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
            handleMessage({ topic: msg.topic, data: fromWire(msg.sample) });
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
        case TOPIC.SENSOR_READING:
            updateSensor(data);
            break;
        case TOPIC.MACHINE_STATUS:
            updateMachine(data);
            break;
        case TOPIC.ALERT:
            addAlert(data);
            break;
        case TOPIC.PREDICTION:
            updatePrediction(data);
            break;
    }
}

/**
 * Update machine card
 */
function updateMachine(data) {
    machines.set(data.machine_id, data);
    renderMachines();
    updateStats();
}

/**
 * Update sensor reading
 */
function updateSensor(data) {
    const key = `${data.machine_id}-${data.sensor_id}`;
    sensors.set(key, data);
    renderSensors();
    updateStats();
}

/**
 * Add alert
 */
function addAlert(data) {
    alerts.unshift({
        ...data,
        time: new Date()
    });

    // Keep only last 20 alerts
    if (alerts.length > 20) {
        alerts.pop();
    }

    alertCount++;
    renderAlerts();
    updateStats();
}

/**
 * Update prediction
 */
function updatePrediction(data) {
    predictions.set(data.machine_id, data);
    renderPredictions();
}

/**
 * Render machines list
 */
function renderMachines() {
    machinesList.innerHTML = '';

    for (const [id, machine] of machines) {
        const card = document.createElement('div');
        card.className = `machine-card ${selectedMachineId === id ? 'selected' : ''}`;
        card.onclick = () => selectMachine(id);

        const statusClass = MACHINE_STATUS[machine.status] || 'stopped';
        const statusText = statusClass.charAt(0).toUpperCase() + statusClass.slice(1);

        const healthColor = machine.health_score > 80 ? 'var(--status-running)' :
                           machine.health_score > 50 ? 'var(--status-warning)' :
                           'var(--status-critical)';

        card.innerHTML = `
            <div class="machine-header">
                <span class="machine-name">${machine.name}</span>
                <span class="machine-status ${statusClass}">${statusText}</span>
            </div>
            <div class="machine-health">
                <div class="health-bar">
                    <div class="health-fill" style="width: ${machine.health_score}%; background: ${healthColor}"></div>
                </div>
            </div>
            <div class="machine-meta">
                <span>Health: ${machine.health_score}%</span>
                <span>${machine.operating_hours.toFixed(0)}h</span>
            </div>
        `;

        machinesList.appendChild(card);
    }

    totalMachinesEl.textContent = machines.size;
}

/**
 * Select machine to filter sensors
 */
function selectMachine(machineId) {
    selectedMachineId = selectedMachineId === machineId ? null : machineId;
    renderMachines();
    renderSensors();
}

/**
 * Render sensor gauges
 */
function renderSensors() {
    sensorsGrid.innerHTML = '';

    for (const [key, sensor] of sensors) {
        // Filter by selected machine
        if (selectedMachineId !== null && sensor.machine_id !== selectedMachineId) {
            continue;
        }

        const card = document.createElement('div');
        const sensorType = SENSOR_TYPES[sensor.sensor_type] || { name: 'Unknown', unit: '', color: '#58a6ff' };
        const machine = machines.get(sensor.machine_id);
        const machineName = machine ? machine.name : `Machine ${sensor.machine_id}`;

        // Calculate percentage for gauge
        const range = sensor.max_threshold - sensor.min_threshold;
        const percentage = Math.min(100, Math.max(0, ((sensor.value - sensor.min_threshold) / range) * 100));

        // Determine status
        const warningThreshold = sensor.max_threshold * 0.9;
        const isWarning = sensor.value > sensor.max_threshold * 0.8;
        const isCritical = sensor.value > sensor.max_threshold * 0.95;

        card.className = `sensor-card ${isCritical ? 'critical' : isWarning ? 'warning' : ''}`;

        // SVG gauge
        const radius = 45;
        const circumference = 2 * Math.PI * radius;
        const offset = circumference - (percentage / 100) * circumference;
        const gaugeColor = isCritical ? 'var(--status-critical)' :
                          isWarning ? 'var(--status-warning)' : sensorType.color;

        card.innerHTML = `
            <div class="sensor-type">${sensorType.name}</div>
            <div class="sensor-machine">${machineName}</div>
            <div class="gauge">
                <svg width="120" height="120" viewBox="0 0 120 120">
                    <circle class="gauge-bg" cx="60" cy="60" r="${radius}"></circle>
                    <circle class="gauge-fill" cx="60" cy="60" r="${radius}"
                        stroke="${gaugeColor}"
                        stroke-dasharray="${circumference}"
                        stroke-dashoffset="${offset}">
                    </circle>
                </svg>
                <div class="gauge-value">
                    ${sensor.value.toFixed(1)}
                    <div class="gauge-unit">${sensorType.unit}</div>
                </div>
            </div>
            <div class="sensor-thresholds">
                <span>Min: ${sensor.min_threshold}</span>
                <span>Max: ${sensor.max_threshold}</span>
            </div>
        `;

        sensorsGrid.appendChild(card);
    }

    activeSensorsEl.textContent = sensors.size;
}

/**
 * Render alerts
 */
function renderAlerts() {
    alertsList.innerHTML = '';

    for (const alert of alerts.slice(0, 10)) {
        const item = document.createElement('div');
        const severityClass = ['info', 'warning', 'critical'][alert.severity] || 'info';

        item.className = `alert-item ${severityClass}`;
        item.innerHTML = `
            <div class="alert-header">
                <span class="alert-type">${alert.alert_type}</span>
                <span class="alert-time">${formatTime(alert.time)}</span>
            </div>
            <div class="alert-message">${alert.message}</div>
        `;

        alertsList.appendChild(item);
    }

    alertCountEl.textContent = alertCount;
}

/**
 * Render predictions
 */
function renderPredictions() {
    predictionsList.innerHTML = '';

    for (const [machineId, prediction] of predictions) {
        const machine = machines.get(machineId);
        const machineName = machine ? machine.name : `Machine ${machineId}`;

        const item = document.createElement('div');
        item.className = 'prediction-item';
        item.innerHTML = `
            <div class="prediction-header">
                <span class="prediction-machine">${machineName}</span>
                <span class="prediction-confidence">${prediction.confidence}% conf</span>
            </div>
            <div class="prediction-failure">${prediction.failure_type.replace(/_/g, ' ')}</div>
            <div class="prediction-time">Estimated failure in ${prediction.hours_to_failure.toFixed(1)} hours</div>
            <div class="prediction-recommendation">${prediction.recommendation}</div>
        `;

        predictionsList.appendChild(item);
    }
}

/**
 * Update stats
 */
function updateStats() {
    if (machines.size > 0) {
        const totalHealth = Array.from(machines.values()).reduce((sum, m) => sum + m.health_score, 0);
        avgHealthEl.textContent = `${Math.round(totalHealth / machines.size)}%`;
    }
}

/**
 * Update connection UI
 */
function updateConnectionUI(isConnected) {
    statusDot.classList.toggle('online', isConnected);
    statusText.textContent = isConnected ? 'Connected' : 'Disconnected';
}

/**
 * Update clock
 */
function updateClock() {
    currentTime.textContent = new Date().toLocaleTimeString('en-US', { hour12: false });
}

/**
 * Format time for alerts
 */
function formatTime(date) {
    return date.toLocaleTimeString('en-US', { hour12: false });
}

/**
 * Demo mode - simulate data
 */
function startDemoMode() {
    updateConnectionUI(true);
    statusText.textContent = 'Demo Mode';

    // Demo machines
    const demoMachines = [
        { machine_id: 1, name: 'Pump Station A', status: 1, health_score: 95, operating_hours: 2341, sensor_count: 4 },
        { machine_id: 2, name: 'Compressor B', status: 1, health_score: 82, operating_hours: 4521, sensor_count: 4 },
        { machine_id: 3, name: 'Motor Drive C', status: 2, health_score: 68, operating_hours: 1823, sensor_count: 4 },
        { machine_id: 4, name: 'Heat Exchanger D', status: 1, health_score: 91, operating_hours: 3245, sensor_count: 3 },
    ];

    // Demo sensors
    const demoSensors = [
        { sensor_id: 101, machine_id: 1, sensor_type: 0, value: 45, min_threshold: 20, max_threshold: 80 },
        { sensor_id: 102, machine_id: 1, sensor_type: 1, value: 2.5, min_threshold: 0, max_threshold: 10 },
        { sensor_id: 103, machine_id: 1, sensor_type: 2, value: 4.5, min_threshold: 2, max_threshold: 8 },
        { sensor_id: 104, machine_id: 1, sensor_type: 3, value: 15, min_threshold: 0, max_threshold: 25 },
        { sensor_id: 201, machine_id: 2, sensor_type: 0, value: 72, min_threshold: 30, max_threshold: 100 },
        { sensor_id: 202, machine_id: 2, sensor_type: 1, value: 6.2, min_threshold: 0, max_threshold: 15 },
        { sensor_id: 203, machine_id: 2, sensor_type: 2, value: 11, min_threshold: 5, max_threshold: 20 },
        { sensor_id: 204, machine_id: 2, sensor_type: 4, value: 3100, min_threshold: 1000, max_threshold: 4000 },
        { sensor_id: 301, machine_id: 3, sensor_type: 0, value: 78, min_threshold: 25, max_threshold: 90 },
        { sensor_id: 302, machine_id: 3, sensor_type: 1, value: 5.8, min_threshold: 0, max_threshold: 8 },
        { sensor_id: 303, machine_id: 3, sensor_type: 3, value: 62, min_threshold: 0, max_threshold: 80 },
        { sensor_id: 304, machine_id: 3, sensor_type: 4, value: 1720, min_threshold: 500, max_threshold: 2000 },
        { sensor_id: 401, machine_id: 4, sensor_type: 0, value: 78, min_threshold: 40, max_threshold: 120 },
        { sensor_id: 402, machine_id: 4, sensor_type: 2, value: 3.2, min_threshold: 1, max_threshold: 6 },
        { sensor_id: 403, machine_id: 4, sensor_type: 5, value: 145, min_threshold: 50, max_threshold: 250 },
    ];

    // Initialize
    demoMachines.forEach(m => handleMessage({ topic: TOPIC.MACHINE_STATUS, data: m }));
    demoSensors.forEach(s => handleMessage({ topic: TOPIC.SENSOR_READING, data: s }));

    // Demo prediction
    handleMessage({
        topic: TOPIC.PREDICTION,
        data: {
            prediction_id: 1,
            machine_id: 3,
            failure_type: 'BEARING_WEAR',
            hours_to_failure: 48.5,
            confidence: 87,
            recommendation: 'Schedule bearing replacement within 48 hours'
        }
    });

    // Demo alert
    handleMessage({
        topic: TOPIC.ALERT,
        data: {
            alert_id: 1,
            machine_id: 3,
            sensor_id: 301,
            severity: 1,
            alert_type: 'HIGH_VALUE',
            message: 'Motor Drive C Temperature high (78°C)'
        }
    });

    // Simulation loop
    setInterval(() => {
        // Update sensor values with noise
        for (const sensor of demoSensors) {
            const range = sensor.max_threshold - sensor.min_threshold;
            const noise = (Math.random() - 0.5) * range * 0.05;
            sensor.value = Math.max(sensor.min_threshold, Math.min(sensor.max_threshold, sensor.value + noise));
            handleMessage({ topic: TOPIC.SENSOR_READING, data: { ...sensor } });
        }

        // Update machine health
        for (const machine of demoMachines) {
            if (machine.machine_id === 3) {
                machine.health_score = Math.max(50, machine.health_score - 0.1);
            }
            machine.operating_hours += 0.01;
            handleMessage({ topic: TOPIC.MACHINE_STATUS, data: { ...machine } });
        }

        // Random alerts
        if (Math.random() < 0.05) {
            const sensor = demoSensors[Math.floor(Math.random() * demoSensors.length)];
            const machine = demoMachines.find(m => m.machine_id === sensor.machine_id);
            handleMessage({
                topic: TOPIC.ALERT,
                data: {
                    alert_id: Date.now(),
                    machine_id: sensor.machine_id,
                    sensor_id: sensor.sensor_id,
                    severity: Math.random() < 0.3 ? 2 : 1,
                    alert_type: 'SENSOR_FLUCTUATION',
                    message: `${machine.name} sensor ${sensor.sensor_id} fluctuation detected`
                }
            });
        }
    }, 200);
}

// Initialize on DOM ready
document.addEventListener('DOMContentLoaded', init);


// --- wire form of the enum fields -------------------------------------------
// The hdds-ws bridge delivers a field declared as an IDL enum as its label,
// while the code above works on the ordinals of that enum. Incoming samples
// are converted once, here; a field that already carries a number is left
// alone, and so are the viewer's own demo-mode samples, which never pass
// through this function.
const WIRE_ENUMS = {
    sensor_type: ['TEMPERATURE', 'VIBRATION', 'PRESSURE', 'CURRENT', 'RPM', 'FLOW_RATE'],
    status: ['STOPPED', 'RUNNING', 'MACHINE_WARNING', 'MACHINE_CRITICAL', 'MAINTENANCE'],
    severity: ['INFO', 'ALERT_WARNING', 'ALERT_CRITICAL'],
};

function fromWire(sample) {
    if (!sample || typeof sample !== 'object') return sample;
    for (const [field, labels] of Object.entries(WIRE_ENUMS)) {
        const value = sample[field];
        if (typeof value !== 'string') continue;
        const ordinal = labels.indexOf(value);
        if (ordinal >= 0) sample[field] = ordinal;
    }
    return sample;
}
