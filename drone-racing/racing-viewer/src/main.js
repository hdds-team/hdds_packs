/**
 * Drone Racing - 3D Viewer with Three.js
 *
 * Connects to HDDS via hdds-ws bridge (WebSocket JSON protocol).
 */

import * as THREE from 'three';

// Configuration
const HDDS_WS_URL = `ws://${window.location.hostname || 'localhost'}:9090/ws`;
const DEMO_MODE = new URLSearchParams(window.location.search).has('demo');
// Topic names, mirroring the `topics` module of racing-types.
const TOPIC = {
    DRONE: 'Drone',
    GATE: 'Gate',
    RACE_STATE: 'RaceState',
    GATE_PASS: 'GatePass',
    LEADERBOARD: 'LeaderboardEntry',
};
const TOPICS = Object.values(TOPIC);

// State
let ws = null;
let connected = false;
const drones = new Map();
const gates = new Map();
let raceState = { phase: 0, countdown: 0, total_laps: 3, elapsed_ms: 0 };
let followDroneId = null;

// Three.js
let scene, camera, renderer;
let droneObjects = new Map();
let gateObjects = [];
let ground, sky;

// DOM
const container = document.getElementById('game-container');
const countdownEl = document.getElementById('countdown');
const raceTimeEl = document.getElementById('race-time');
const speedEl = document.getElementById('speed');
const positionEl = document.getElementById('position');
const positionSuffixEl = document.getElementById('position-suffix');
const currentLapEl = document.getElementById('current-lap');
const totalLapsEl = document.getElementById('total-laps');
const leaderboardList = document.getElementById('leaderboard-list');
const minimapCanvas = document.getElementById('minimap-canvas');
const minimapCtx = minimapCanvas.getContext('2d');
const statusDot = document.querySelector('.status-dot');
const statusText = document.querySelector('.status-text');

/**
 * Initialize
 */
function init() {
    console.log('[RACING] Initializing Drone Racing 3D');

    initThreeJS();

    if (DEMO_MODE) {
        startDemoMode();
    } else {
        connectWebSocket();
    }

    animate();
}

/**
 * Initialize Three.js scene
 */
function initThreeJS() {
    // Scene
    scene = new THREE.Scene();
    scene.fog = new THREE.Fog(0x0a0a2a, 100, 500);

    // Camera
    camera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 1000);
    camera.position.set(0, 50, -100);
    camera.lookAt(0, 0, 0);

    // Renderer
    renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(window.innerWidth, window.innerHeight);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setClearColor(0x0a0a2a);
    container.appendChild(renderer.domElement);

    // Lights
    const ambientLight = new THREE.AmbientLight(0x404080, 0.5);
    scene.add(ambientLight);

    const directionalLight = new THREE.DirectionalLight(0xffffff, 1);
    directionalLight.position.set(50, 100, 50);
    scene.add(directionalLight);

    // Ground
    const groundGeometry = new THREE.PlaneGeometry(1000, 1000, 50, 50);
    const groundMaterial = new THREE.MeshStandardMaterial({
        color: 0x1a1a3a,
        wireframe: false,
        roughness: 0.8,
    });
    ground = new THREE.Mesh(groundGeometry, groundMaterial);
    ground.rotation.x = -Math.PI / 2;
    ground.position.y = 0;
    scene.add(ground);

    // Grid helper
    const gridHelper = new THREE.GridHelper(1000, 100, 0x00ffff, 0x004444);
    gridHelper.position.y = 0.1;
    scene.add(gridHelper);

    // Skybox (simple gradient)
    const skyGeometry = new THREE.SphereGeometry(400, 32, 32);
    const skyMaterial = new THREE.ShaderMaterial({
        uniforms: {
            topColor: { value: new THREE.Color(0x0a0a2a) },
            bottomColor: { value: new THREE.Color(0x1a1a4a) },
        },
        vertexShader: `
            varying vec3 vWorldPosition;
            void main() {
                vec4 worldPosition = modelMatrix * vec4(position, 1.0);
                vWorldPosition = worldPosition.xyz;
                gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
            }
        `,
        fragmentShader: `
            uniform vec3 topColor;
            uniform vec3 bottomColor;
            varying vec3 vWorldPosition;
            void main() {
                float h = normalize(vWorldPosition).y;
                gl_FragColor = vec4(mix(bottomColor, topColor, max(h, 0.0)), 1.0);
            }
        `,
        side: THREE.BackSide,
    });
    sky = new THREE.Mesh(skyGeometry, skyMaterial);
    scene.add(sky);

    // Handle resize
    window.addEventListener('resize', onWindowResize);
}

function onWindowResize() {
    camera.aspect = window.innerWidth / window.innerHeight;
    camera.updateProjectionMatrix();
    renderer.setSize(window.innerWidth, window.innerHeight);
}

/**
 * Create drone 3D object
 */
function createDroneObject(drone) {
    const group = new THREE.Group();

    // Body
    const bodyGeometry = new THREE.BoxGeometry(2, 0.5, 3);
    const bodyMaterial = new THREE.MeshStandardMaterial({
        color: drone.color,
        emissive: drone.color,
        emissiveIntensity: 0.3,
        metalness: 0.8,
        roughness: 0.2,
    });
    const body = new THREE.Mesh(bodyGeometry, bodyMaterial);
    group.add(body);

    // Arms (4 arms for quadcopter)
    const armGeometry = new THREE.BoxGeometry(0.3, 0.2, 2);
    const armMaterial = new THREE.MeshStandardMaterial({ color: 0x333333 });

    const armPositions = [
        { x: 1.5, z: 1.5, rot: Math.PI / 4 },
        { x: -1.5, z: 1.5, rot: -Math.PI / 4 },
        { x: 1.5, z: -1.5, rot: -Math.PI / 4 },
        { x: -1.5, z: -1.5, rot: Math.PI / 4 },
    ];

    for (const pos of armPositions) {
        const arm = new THREE.Mesh(armGeometry, armMaterial);
        arm.position.set(pos.x, 0, pos.z);
        arm.rotation.y = pos.rot;
        group.add(arm);

        // Rotor
        const rotorGeometry = new THREE.CylinderGeometry(0.8, 0.8, 0.1, 16);
        const rotorMaterial = new THREE.MeshStandardMaterial({
            color: 0x00ffff,
            emissive: 0x00ffff,
            emissiveIntensity: 0.5,
            transparent: true,
            opacity: 0.7,
        });
        const rotor = new THREE.Mesh(rotorGeometry, rotorMaterial);
        rotor.position.set(pos.x * 1.2, 0.3, pos.z * 1.2);
        rotor.userData.isRotor = true;
        group.add(rotor);
    }

    // Trail light
    const trailGeometry = new THREE.SphereGeometry(0.5, 8, 8);
    const trailMaterial = new THREE.MeshBasicMaterial({
        color: drone.color,
        transparent: true,
        opacity: 0.8,
    });
    const trail = new THREE.Mesh(trailGeometry, trailMaterial);
    trail.position.z = -2;
    group.add(trail);

    // Point light
    const light = new THREE.PointLight(drone.color, 1, 20);
    light.position.y = 1;
    group.add(light);

    return group;
}

/**
 * Create gate 3D object
 */
function createGateObject(gate) {
    const group = new THREE.Group();

    const isStart = gate.is_start_finish === 1;
    const color = isStart ? 0x00ff00 : 0x00ffff;

    // Gate frame (torus)
    const frameGeometry = new THREE.TorusGeometry(gate.width / 2, 0.5, 8, 32);
    const frameMaterial = new THREE.MeshStandardMaterial({
        color: color,
        emissive: color,
        emissiveIntensity: 0.5,
        metalness: 0.9,
        roughness: 0.1,
    });
    const frame = new THREE.Mesh(frameGeometry, frameMaterial);
    frame.rotation.y = Math.PI / 2;
    group.add(frame);

    // Gate number
    // (Would need TextGeometry or sprites for text)

    // Glow effect
    const glowGeometry = new THREE.TorusGeometry(gate.width / 2, 1, 8, 32);
    const glowMaterial = new THREE.MeshBasicMaterial({
        color: color,
        transparent: true,
        opacity: 0.2,
    });
    const glow = new THREE.Mesh(glowGeometry, glowMaterial);
    glow.rotation.y = Math.PI / 2;
    group.add(glow);

    group.position.set(gate.x, gate.y, gate.z);
    group.rotation.y = gate.rotation;

    return group;
}

/**
 * Connect WebSocket
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

    ws.onerror = (e) => console.error('[WS] Error:', e);

    ws.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            handleWsMessage(msg);
        } catch (e) {
            console.warn('[WS] Parse error:', e);
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
        case TOPIC.DRONE:
            drones.set(data.drone_id, data);
            updateDroneObject(data);
            if (followDroneId === null) {
                followDroneId = data.drone_id;
            }
            break;

        case TOPIC.GATE:
            gates.set(data.gate_id, data);
            if (!gateObjects[data.gate_id]) {
                const obj = createGateObject(data);
                scene.add(obj);
                gateObjects[data.gate_id] = obj;
            }
            break;

        case TOPIC.RACE_STATE:
            raceState = data;
            updateRaceUI();
            break;

        case TOPIC.GATE_PASS:
            showGatePass(data);
            break;

        case TOPIC.LEADERBOARD:
            updateLeaderboard(data);
            break;
    }
}

/**
 * Update drone 3D object
 */
function updateDroneObject(drone) {
    let obj = droneObjects.get(drone.drone_id);

    if (!obj) {
        obj = createDroneObject(drone);
        scene.add(obj);
        droneObjects.set(drone.drone_id, obj);
    }

    // Update position
    obj.position.set(drone.x, drone.y, drone.z);

    // Update rotation
    obj.rotation.set(drone.pitch, drone.yaw, drone.roll);

    // Animate rotors based on throttle
    obj.children.forEach(child => {
        if (child.userData.isRotor) {
            child.rotation.y += drone.throttle * 0.5;
        }
    });
}

/**
 * Update race UI
 */
function updateRaceUI() {
    // Countdown
    if (raceState.phase === 1 && raceState.countdown > 0) {
        countdownEl.textContent = raceState.countdown;
        countdownEl.style.display = 'block';
    } else if (raceState.phase === 1 && raceState.countdown === 0) {
        countdownEl.textContent = 'GO!';
        setTimeout(() => countdownEl.style.display = 'none', 500);
    } else {
        countdownEl.style.display = 'none';
    }

    // Race time
    const ms = raceState.elapsed_ms;
    const mins = Math.floor(ms / 60000);
    const secs = Math.floor((ms % 60000) / 1000);
    const millis = ms % 1000;
    raceTimeEl.textContent = `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}.${millis.toString().padStart(3, '0')}`;

    totalLapsEl.textContent = raceState.total_laps;

    // Update followed drone info
    if (followDroneId && drones.has(followDroneId)) {
        const drone = drones.get(followDroneId);
        const speed = Math.sqrt(drone.vx ** 2 + drone.vy ** 2 + drone.vz ** 2) * 3.6; // m/s to km/h
        speedEl.textContent = Math.round(speed);

        positionEl.textContent = drone.position;
        positionSuffixEl.textContent = getPositionSuffix(drone.position);

        currentLapEl.textContent = drone.lap;
    }
}

function getPositionSuffix(pos) {
    if (pos === 1) return 'st';
    if (pos === 2) return 'nd';
    if (pos === 3) return 'rd';
    return 'th';
}

/**
 * Show gate pass effect
 */
function showGatePass(pass) {
    const flash = document.createElement('div');
    flash.className = 'gate-flash';
    flash.textContent = `GATE ${pass.gate_id + 1}`;
    document.body.appendChild(flash);
    setTimeout(() => flash.remove(), 500);
}

/**
 * Update leaderboard
 */
function updateLeaderboard(entry) {
    let el = document.getElementById(`lb-${entry.drone_id}`);
    if (!el) {
        el = document.createElement('div');
        el.id = `lb-${entry.drone_id}`;
        el.className = 'leaderboard-entry';
        leaderboardList.appendChild(el);
    }

    const drone = drones.get(entry.drone_id);
    const color = drone ? `#${drone.color.toString(16).padStart(6, '0')}` : '#ffffff';
    const isSelf = entry.drone_id === followDroneId;

    el.className = `leaderboard-entry ${isSelf ? 'self' : ''}`;
    el.innerHTML = `
        <span class="entry-position">${entry.position}</span>
        <span class="entry-color" style="background: ${color}"></span>
        <span class="entry-name">${entry.drone_name}</span>
        <span class="entry-time">L${entry.lap} G${entry.gate}</span>
    `;

    // Sort leaderboard
    const entries = Array.from(leaderboardList.children);
    entries.sort((a, b) => {
        const posA = parseInt(a.querySelector('.entry-position').textContent);
        const posB = parseInt(b.querySelector('.entry-position').textContent);
        return posA - posB;
    });
    entries.forEach(e => leaderboardList.appendChild(e));
}

/**
 * Update connection UI
 */
function updateConnectionUI(isConnected) {
    statusDot.classList.toggle('connected', isConnected);
    statusText.textContent = isConnected ? 'Connected' : 'Disconnected';
}

/**
 * Update minimap
 */
function updateMinimap() {
    const ctx = minimapCtx;
    const w = minimapCanvas.width;
    const h = minimapCanvas.height;
    const scale = 0.4;
    const offsetX = w / 2;
    const offsetY = h / 2;

    ctx.fillStyle = 'rgba(0, 20, 40, 0.9)';
    ctx.fillRect(0, 0, w, h);

    // Draw track (gates connected)
    ctx.strokeStyle = '#004466';
    ctx.lineWidth = 2;
    ctx.beginPath();
    const gateArray = Array.from(gates.values()).sort((a, b) => a.gate_id - b.gate_id);
    for (let i = 0; i < gateArray.length; i++) {
        const gate = gateArray[i];
        const x = offsetX + gate.x * scale;
        const y = offsetY + gate.z * scale;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
    }
    if (gateArray.length > 0) {
        ctx.lineTo(offsetX + gateArray[0].x * scale, offsetY + gateArray[0].z * scale);
    }
    ctx.stroke();

    // Draw gates
    for (const gate of gates.values()) {
        const x = offsetX + gate.x * scale;
        const y = offsetY + gate.z * scale;

        ctx.fillStyle = gate.is_start_finish ? '#00ff00' : '#00ffff';
        ctx.beginPath();
        ctx.arc(x, y, 4, 0, Math.PI * 2);
        ctx.fill();
    }

    // Draw drones
    for (const drone of drones.values()) {
        const x = offsetX + drone.x * scale;
        const y = offsetY + drone.z * scale;
        const color = `#${drone.color.toString(16).padStart(6, '0')}`;

        ctx.fillStyle = color;
        ctx.beginPath();
        ctx.arc(x, y, drone.drone_id === followDroneId ? 6 : 4, 0, Math.PI * 2);
        ctx.fill();

        // Direction indicator
        const dirX = Math.sin(drone.yaw) * 8;
        const dirY = Math.cos(drone.yaw) * 8;
        ctx.strokeStyle = color;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(x, y);
        ctx.lineTo(x + dirX, y + dirY);
        ctx.stroke();
    }
}

/**
 * Animation loop
 */
function animate() {
    requestAnimationFrame(animate);

    // Follow camera
    if (followDroneId && drones.has(followDroneId)) {
        const drone = drones.get(followDroneId);

        // Chase camera
        const cameraDistance = 30;
        const cameraHeight = 10;

        const targetX = drone.x - Math.sin(drone.yaw) * cameraDistance;
        const targetY = drone.y + cameraHeight;
        const targetZ = drone.z - Math.cos(drone.yaw) * cameraDistance;

        camera.position.lerp(new THREE.Vector3(targetX, targetY, targetZ), 0.05);
        camera.lookAt(drone.x, drone.y, drone.z);
    }

    // Update minimap
    updateMinimap();

    renderer.render(scene, camera);
}

/**
 * Demo mode
 */
function startDemoMode() {
    updateConnectionUI(true);
    statusText.textContent = 'Demo Mode';

    // Create demo gates
    const gatePositions = [
        { gate_id: 0, x: 0, y: 10, z: 0, rotation: 0, width: 20, height: 15, is_start_finish: 1 },
        { gate_id: 1, x: 80, y: 15, z: 50, rotation: 0.3, width: 20, height: 15, is_start_finish: 0 },
        { gate_id: 2, x: 150, y: 25, z: 120, rotation: 0.5, width: 20, height: 15, is_start_finish: 0 },
        { gate_id: 3, x: 180, y: 30, z: 200, rotation: Math.PI * 0.3, width: 20, height: 15, is_start_finish: 0 },
        { gate_id: 4, x: 150, y: 35, z: 280, rotation: Math.PI * 0.5, width: 20, height: 15, is_start_finish: 0 },
        { gate_id: 5, x: 80, y: 30, z: 320, rotation: Math.PI * 0.7, width: 20, height: 15, is_start_finish: 0 },
        { gate_id: 6, x: 0, y: 25, z: 350, rotation: Math.PI, width: 20, height: 15, is_start_finish: 0 },
        { gate_id: 7, x: -80, y: 20, z: 300, rotation: Math.PI * 1.3, width: 20, height: 15, is_start_finish: 0 },
        { gate_id: 8, x: -120, y: 15, z: 200, rotation: Math.PI * 1.5, width: 20, height: 15, is_start_finish: 0 },
        { gate_id: 9, x: -80, y: 10, z: 100, rotation: Math.PI * 1.8, width: 20, height: 15, is_start_finish: 0 },
    ];

    gatePositions.forEach(g => handleMessage({ topic: TOPIC.GATE, data: g }));

    // Create demo drones
    const demoDrones = [
        { drone_id: 1, name: 'PHOENIX', color: 0xFF0066, x: -10, y: 10, z: 0, yaw: 0, lap: 1, position: 1 },
        { drone_id: 2, name: 'VIPER', color: 0x00FF66, x: 0, y: 12, z: -5, yaw: 0, lap: 1, position: 2 },
        { drone_id: 3, name: 'STORM', color: 0x6600FF, x: 10, y: 11, z: 0, yaw: 0, lap: 1, position: 3 },
    ];

    demoDrones.forEach(d => {
        d.vx = 0; d.vy = 0; d.vz = 0;
        d.pitch = 0; d.roll = 0;
        d.state = 2; d.throttle = 0.8;
        d.current_gate = 0;
        d.best_lap_ms = 0;
        d.total_time_ms = 0;
        handleMessage({ topic: TOPIC.DRONE, data: d });
    });

    raceState = { phase: 2, countdown: 0, total_laps: 3, elapsed_ms: 0 };
    updateRaceUI();

    followDroneId = 1;

    // Animate demo
    let time = 0;
    setInterval(() => {
        time += 0.016;
        raceState.elapsed_ms += 16;

        for (const drone of demoDrones) {
            // Follow track
            const gateIdx = drone.current_gate || 0;
            const gate = gatePositions[gateIdx];

            const dx = gate.x - drone.x;
            const dy = gate.y - drone.y;
            const dz = gate.z - drone.z;
            const dist = Math.sqrt(dx * dx + dy * dy + dz * dz);

            drone.yaw = Math.atan2(dx, dz);
            drone.pitch = -Math.atan2(dy, Math.sqrt(dx * dx + dz * dz)) * 0.3;
            drone.roll = Math.sin(time * 2 + drone.drone_id) * 0.2;

            const speed = 40 + drone.drone_id * 5;
            drone.vx = Math.sin(drone.yaw) * speed * 0.016;
            drone.vy = -Math.sin(drone.pitch) * speed * 0.016;
            drone.vz = Math.cos(drone.yaw) * speed * 0.016;

            drone.x += drone.vx;
            drone.y += drone.vy;
            drone.z += drone.vz;

            if (dist < 20) {
                drone.current_gate = (drone.current_gate + 1) % gatePositions.length;
                if (drone.current_gate === 0) drone.lap++;
            }

            handleMessage({ topic: TOPIC.DRONE, data: { ...drone } });
        }

        updateRaceUI();
    }, 16);
}

// Start
document.addEventListener('DOMContentLoaded', init);


// --- wire form of the enum fields -------------------------------------------
// The hdds-ws bridge delivers a field declared as an IDL enum as its label,
// while the code above works on the ordinals of that enum. Incoming samples
// are converted once, here; a field that already carries a number is left
// alone, and so are the viewer's own demo-mode samples, which never pass
// through this function.
const WIRE_ENUMS = {
    phase: ['WAITING', 'COUNTDOWN', 'PHASE_RACING', 'PHASE_FINISHED'],
    state: ['IDLE', 'READY', 'DRONE_RACING', 'CRASHED', 'DRONE_FINISHED'],
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
