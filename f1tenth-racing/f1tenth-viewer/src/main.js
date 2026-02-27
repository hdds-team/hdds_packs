/**
 * F1Tenth Racing - 3D Viewer with Three.js
 *
 * Connects to HDDS via hdds-ws bridge (WebSocket JSON protocol).
 * Top-down / chase camera, low-poly F1Tenth cars, real-time track.
 */

import * as THREE from 'three';

// --- Config ---
const WS_URL = `ws://${window.location.hostname || 'localhost'}:9090/ws`;
const DEMO_MODE = new URLSearchParams(window.location.search).has('demo');

// --- State ---
let ws = null;
let connected = false;
const cars = new Map();         // car_id -> Car data
const trackPoints = [];         // sorted TrackPoint[]
let raceState = { phase: 0, countdown: 0, total_laps: 5, elapsed_ms: 0 };
let followCarId = 1;            // which car to follow (0 = free cam)

// --- Three.js ---
let scene, camera, renderer, clock;
const carMeshes = new Map();    // car_id -> THREE.Group
const carTrails = new Map();    // car_id -> trail points
let trackMesh = null;
let trackLine = null;

// --- DOM ---
const container = document.getElementById('scene-container');
const bannerEl = document.getElementById('race-banner');
const raceTimeEl = document.getElementById('race-time');
const leaderboardList = document.getElementById('leaderboard-list');
const carNameEl = document.getElementById('car-name');
const carSpeedEl = document.getElementById('car-speed');
const carLapEl = document.getElementById('car-lap');
const carBestEl = document.getElementById('car-best');
const carLastEl = document.getElementById('car-last');
const minimapCanvas = document.getElementById('minimap');
const minimapCtx = minimapCanvas.getContext('2d');
const connDot = document.querySelector('.conn-dot');
const connText = document.querySelector('.conn-text');
const sectorEls = [
    document.getElementById('sector-0'),
    document.getElementById('sector-1'),
    document.getElementById('sector-2'),
];

// ===========================================
// Init
// ===========================================
function init() {
    console.log('[F1T] Initializing F1Tenth Racing Viewer');
    initScene();

    if (DEMO_MODE) {
        startDemoMode();
    } else {
        connectWS();
    }

    // Key bindings
    window.addEventListener('keydown', onKey);
    window.addEventListener('resize', onResize);
    window.addEventListener('wheel', onWheel);

    animate();
}

// ===========================================
// Three.js Scene Setup
// ===========================================
function initScene() {
    scene = new THREE.Scene();
    scene.background = new THREE.Color(0x1a1a2e);
    scene.fog = new THREE.FogExp2(0x1a1a2e, 0.015);
    clock = new THREE.Clock();

    // Camera - elevated isometric view
    camera = new THREE.PerspectiveCamera(50, window.innerWidth / window.innerHeight, 0.1, 200);
    camera.position.set(5, 14, 5);
    camera.lookAt(5, 0, 5);

    // Renderer
    renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(window.innerWidth, window.innerHeight);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    container.appendChild(renderer.domElement);

    // Lights
    const ambient = new THREE.AmbientLight(0x404060, 0.6);
    scene.add(ambient);

    const sun = new THREE.DirectionalLight(0xffeedd, 1.2);
    sun.position.set(15, 20, 10);
    sun.castShadow = true;
    sun.shadow.mapSize.width = 2048;
    sun.shadow.mapSize.height = 2048;
    sun.shadow.camera.near = 0.5;
    sun.shadow.camera.far = 60;
    sun.shadow.camera.left = -15;
    sun.shadow.camera.right = 15;
    sun.shadow.camera.top = 15;
    sun.shadow.camera.bottom = -15;
    scene.add(sun);

    // Ground plane
    const groundGeo = new THREE.PlaneGeometry(100, 100);
    const groundMat = new THREE.MeshStandardMaterial({
        color: 0x2a2a3a,
        roughness: 0.95,
    });
    const ground = new THREE.Mesh(groundGeo, groundMat);
    ground.rotation.x = -Math.PI / 2;
    ground.position.y = -0.01;
    ground.receiveShadow = true;
    scene.add(ground);

    // Grid
    const grid = new THREE.GridHelper(100, 200, 0x333355, 0x222244);
    grid.position.y = 0;
    scene.add(grid);
}

// ===========================================
// Car Mesh (low-poly F1Tenth)
// ===========================================
function createCarMesh(carData) {
    const group = new THREE.Group();
    const color = new THREE.Color(carData.color);

    // Body - flat box (F1Tenth is a small car)
    const bodyGeo = new THREE.BoxGeometry(0.18, 0.06, 0.33);
    const bodyMat = new THREE.MeshStandardMaterial({
        color: color,
        metalness: 0.4,
        roughness: 0.3,
    });
    const body = new THREE.Mesh(bodyGeo, bodyMat);
    body.position.y = 0.06;
    body.castShadow = true;
    group.add(body);

    // Nose (wedge)
    const noseGeo = new THREE.BoxGeometry(0.14, 0.04, 0.08);
    const noseMat = new THREE.MeshStandardMaterial({
        color: color.clone().multiplyScalar(0.8),
        metalness: 0.5,
        roughness: 0.2,
    });
    const nose = new THREE.Mesh(noseGeo, noseMat);
    nose.position.set(0, 0.05, 0.19);
    nose.castShadow = true;
    group.add(nose);

    // Rear wing
    const wingGeo = new THREE.BoxGeometry(0.22, 0.04, 0.02);
    const wingMat = new THREE.MeshStandardMaterial({ color: 0x222222 });
    const wing = new THREE.Mesh(wingGeo, wingMat);
    wing.position.set(0, 0.12, -0.14);
    group.add(wing);

    // Wing endplates
    for (const side of [-1, 1]) {
        const plateGeo = new THREE.BoxGeometry(0.01, 0.06, 0.04);
        const plate = new THREE.Mesh(plateGeo, wingMat);
        plate.position.set(side * 0.11, 0.10, -0.14);
        group.add(plate);
    }

    // Wheels (4 cylinders)
    const wheelGeo = new THREE.CylinderGeometry(0.03, 0.03, 0.02, 8);
    const wheelMat = new THREE.MeshStandardMaterial({ color: 0x111111 });

    const wheelPositions = [
        [-0.10, 0.03, 0.12],   // front-left
        [0.10, 0.03, 0.12],    // front-right
        [-0.10, 0.03, -0.12],  // rear-left
        [0.10, 0.03, -0.12],   // rear-right
    ];

    for (const [wx, wy, wz] of wheelPositions) {
        const wheel = new THREE.Mesh(wheelGeo, wheelMat);
        wheel.rotation.z = Math.PI / 2;
        wheel.position.set(wx, wy, wz);
        group.add(wheel);
    }

    // Number label (sprite)
    const canvas = document.createElement('canvas');
    canvas.width = 64;
    canvas.height = 64;
    const ctx = canvas.getContext('2d');
    ctx.fillStyle = '#000';
    ctx.fillRect(0, 0, 64, 64);
    ctx.fillStyle = '#fff';
    ctx.font = 'bold 40px monospace';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(String(carData.car_id), 32, 32);
    const texture = new THREE.CanvasTexture(canvas);
    const spriteMat = new THREE.SpriteMaterial({ map: texture, sizeAttenuation: true });
    const sprite = new THREE.Sprite(spriteMat);
    sprite.scale.set(0.15, 0.15, 1);
    sprite.position.y = 0.20;
    group.add(sprite);

    // Glow point light
    const glow = new THREE.PointLight(color, 0.3, 1.5);
    glow.position.set(0, 0.15, 0);
    group.add(glow);

    scene.add(group);
    return group;
}

// ===========================================
// Track Rendering
// ===========================================
function buildTrack() {
    if (trackPoints.length < 3) return;

    // Remove old
    if (trackMesh) { scene.remove(trackMesh); trackMesh = null; }
    if (trackLine) { scene.remove(trackLine); trackLine = null; }

    // Sort by index
    trackPoints.sort((a, b) => a.index - b.index);

    const n = trackPoints.length;

    // Track surface (extruded ribbon)
    const shape = new THREE.Shape();
    const vertices = [];

    for (let i = 0; i <= n; i++) {
        const pt = trackPoints[i % n];
        const next = trackPoints[(i + 1) % n];
        const prev = trackPoints[(i - 1 + n) % n];

        // Normal to track direction
        const dx = next.x - prev.x;
        const dz = next.z - prev.z;
        const len = Math.sqrt(dx * dx + dz * dz) || 1;
        const nx = -dz / len;
        const nz = dx / len;

        const w = pt.width || 0.6;

        // Left and right edges
        vertices.push(
            pt.x + nx * w, 0.005, pt.z + nz * w,  // left
            pt.x - nx * w, 0.005, pt.z - nz * w,  // right
        );
    }

    // Build geometry from triangle strip
    const geo = new THREE.BufferGeometry();
    const positions = [];
    const indices = [];

    for (let i = 0; i < vertices.length; i += 6) {
        positions.push(
            vertices[i], vertices[i + 1], vertices[i + 2],
            vertices[i + 3], vertices[i + 4], vertices[i + 5],
        );
    }

    const numVerts = positions.length / 3;
    for (let i = 0; i < numVerts - 2; i += 2) {
        indices.push(i, i + 1, i + 2);
        indices.push(i + 1, i + 3, i + 2);
    }
    // Close the loop
    if (numVerts >= 4) {
        indices.push(numVerts - 2, numVerts - 1, 0);
        indices.push(numVerts - 1, 1, 0);
    }

    geo.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
    geo.setIndex(indices);
    geo.computeVertexNormals();

    const mat = new THREE.MeshStandardMaterial({
        color: 0x444466,
        roughness: 0.7,
        side: THREE.DoubleSide,
    });
    trackMesh = new THREE.Mesh(geo, mat);
    trackMesh.receiveShadow = true;
    scene.add(trackMesh);

    // Centerline
    const linePoints = trackPoints.map(p => new THREE.Vector3(p.x, 0.02, p.z));
    linePoints.push(linePoints[0]); // close
    const lineGeo = new THREE.BufferGeometry().setFromPoints(linePoints);
    const lineMat = new THREE.LineBasicMaterial({ color: 0x666688, linewidth: 1 });
    trackLine = new THREE.Line(lineGeo, lineMat);
    scene.add(trackLine);

    // Start/finish line marker
    if (trackPoints.length > 1) {
        const p0 = trackPoints[0];
        const p1 = trackPoints[1];
        const dx = p1.x - p0.x;
        const dz = p1.z - p0.z;
        const len = Math.sqrt(dx * dx + dz * dz) || 1;
        const nx = -dz / len;
        const nz = dx / len;

        const startGeo = new THREE.BufferGeometry().setFromPoints([
            new THREE.Vector3(p0.x + nx * 0.8, 0.03, p0.z + nz * 0.8),
            new THREE.Vector3(p0.x - nx * 0.8, 0.03, p0.z - nz * 0.8),
        ]);
        const startMat = new THREE.LineBasicMaterial({ color: 0xffffff, linewidth: 2 });
        scene.add(new THREE.Line(startGeo, startMat));
    }

    console.log(`[F1T] Track built: ${n} points`);
}

// ===========================================
// WebSocket Connection
// ===========================================
function connectWS() {
    console.log(`[F1T] Connecting to ${WS_URL}`);
    ws = new WebSocket(WS_URL);

    ws.onopen = () => {
        connected = true;
        connDot.classList.add('connected');
        connText.textContent = 'HDDS Connected';
        console.log('[F1T] WebSocket connected');

        // Subscribe to topics (hdds-ws protocol: { type: 'subscribe', topic })
        const subs = [
            'f1tenth/car',
            'f1tenth/track_point',
            'f1tenth/race_state',
            'f1tenth/lap_event',
            'f1tenth/sector_time',
        ];
        for (const topic of subs) {
            ws.send(JSON.stringify({ type: 'subscribe', topic }));
        }
    };

    ws.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            switch (msg.type) {
                case 'welcome':
                    console.log(`[F1T] HDDS domain ${msg.domain}`);
                    break;
                case 'subscribed':
                    console.log(`[F1T] Subscribed to ${msg.topic}`);
                    break;
                case 'data':
                    handleMessage({ topic: msg.topic, data: msg.sample });
                    break;
                case 'error':
                    console.error(`[F1T] Error [${msg.code}]: ${msg.message}`);
                    break;
            }
        } catch (e) {
            // ignore parse errors
        }
    };

    ws.onclose = () => {
        connected = false;
        connDot.classList.remove('connected');
        connText.textContent = 'Disconnected';
        console.log('[F1T] WebSocket closed, reconnecting...');
        setTimeout(connectWS, 2000);
    };

    ws.onerror = () => {
        ws.close();
    };
}

function handleMessage(msg) {
    if (!msg.topic || !msg.data) return;

    switch (msg.topic) {
        case 'f1tenth/car':
            handleCar(msg.data);
            break;
        case 'f1tenth/track_point':
            handleTrackPoint(msg.data);
            break;
        case 'f1tenth/race_state':
            handleRaceState(msg.data);
            break;
        case 'f1tenth/lap_event':
            handleLapEvent(msg.data);
            break;
        case 'f1tenth/sector_time':
            handleSectorTime(msg.data);
            break;
    }
}

function handleCar(data) {
    cars.set(data.car_id, data);

    // Create mesh if needed
    if (!carMeshes.has(data.car_id)) {
        const mesh = createCarMesh(data);
        carMeshes.set(data.car_id, mesh);
        carTrails.set(data.car_id, []);
    }

    // Update mesh position
    const mesh = carMeshes.get(data.car_id);
    mesh.position.set(data.x, data.y || 0, data.z);
    // heading: 0 = +Z, rotation around Y axis
    mesh.rotation.y = -data.heading + Math.PI / 2;

    // Trail
    const trail = carTrails.get(data.car_id);
    trail.push({ x: data.x, z: data.z });
    if (trail.length > 600) trail.shift(); // keep last 10s at 60Hz
}

let trackBuildTimer = null;

function handleTrackPoint(data) {
    // Deduplicate by index
    const existing = trackPoints.findIndex(p => p.index === data.index);
    if (existing >= 0) {
        trackPoints[existing] = data;
    } else {
        trackPoints.push(data);
    }

    // Debounce track rebuild (points arrive in burst at start)
    if (trackBuildTimer) clearTimeout(trackBuildTimer);
    trackBuildTimer = setTimeout(() => {
        if (trackPoints.length > 3) buildTrack();
    }, 200);
}

function handleRaceState(data) {
    raceState = data;
}

function handleLapEvent(data) {
    const car = cars.get(data.car_id);
    const name = car ? car.name : `Car ${data.car_id}`;
    console.log(`[F1T] ${name} lap ${data.lap}: ${formatTime(data.lap_time_ms)}`);
}

function handleSectorTime(data) {
    // Visual feedback on sector elements
    const el = sectorEls[data.sector];
    if (el) {
        el.classList.add('green');
        setTimeout(() => el.classList.remove('green'), 2000);
    }
}

// ===========================================
// Demo Mode (no WebSocket, simulated data)
// ===========================================
function startDemoMode() {
    connDot.classList.add('connected');
    connText.textContent = 'DEMO MODE';

    // Generate a simple oval track
    const n = 80;
    for (let i = 0; i < n; i++) {
        const angle = (i / n) * Math.PI * 2;
        const rx = 4.0;
        const rz = 3.0;
        trackPoints.push({
            index: i,
            x: 5 + Math.cos(angle) * rx,
            z: 5 + Math.sin(angle) * rz,
            width: 0.6,
        });
    }
    buildTrack();

    // Demo cars
    const demoCars = [
        { car_id: 1, name: 'APEX', color: 0xFF2200 },
        { car_id: 2, name: 'PHANTOM', color: 0x00CCFF },
        { car_id: 3, name: 'VORTEX', color: 0xFFCC00 },
        { car_id: 4, name: 'SHADOW', color: 0xAA00FF },
    ];

    const angles = [0, 0.05, 0.10, 0.15];
    const speeds = [0.018, 0.017, 0.0165, 0.016];

    let tick = 0;
    setInterval(() => {
        tick++;
        for (let i = 0; i < demoCars.length; i++) {
            angles[i] += speeds[i] + Math.sin(tick * 0.01 + i) * 0.001;
            const a = angles[i];
            const x = 5 + Math.cos(a) * 4.0;
            const z = 5 + Math.sin(a) * 3.0;
            const heading = a + Math.PI / 2;
            const speed = speeds[i] * 60 * 5;

            handleCar({
                ...demoCars[i],
                x, y: 0, z,
                vx: Math.cos(heading) * speed,
                vz: Math.sin(heading) * speed,
                speed,
                heading,
                steering_angle: 0.1 * Math.sin(tick * 0.05),
                state: 2, // Racing
                lap: Math.floor(angles[i] / (Math.PI * 2)),
                total_laps: 5,
                sector: Math.floor((a % (Math.PI * 2)) / (Math.PI * 2 / 3)),
                track_progress: (a % (Math.PI * 2)) / (Math.PI * 2),
                best_lap_ms: 8000 + i * 500,
                last_lap_ms: 8200 + i * 400 + Math.floor(Math.random() * 500),
                total_time_ms: tick * 16,
                position: i + 1,
            });
        }

        raceState = {
            phase: 2,
            countdown: 0,
            total_laps: 5,
            elapsed_ms: tick * 16,
            car_count: 4,
        };
    }, 16);
}

// ===========================================
// Camera
// ===========================================
let cameraZoom = 14;
let cameraAngle = 0.3; // radians from vertical

function updateCamera() {
    const followed = cars.get(followCarId);

    if (followed && followCarId > 0) {
        // Chase cam slightly behind and above
        const targetX = followed.x;
        const targetZ = followed.z;

        // Smooth follow
        const lerpFactor = 0.08;
        const offsetX = -Math.cos(followed.heading) * 3;
        const offsetZ = -Math.sin(followed.heading) * 3;

        camera.position.x += (targetX + offsetX - camera.position.x) * lerpFactor;
        camera.position.z += (targetZ + offsetZ - camera.position.z) * lerpFactor;
        camera.position.y += (cameraZoom - camera.position.y) * lerpFactor;

        camera.lookAt(targetX, 0, targetZ);
    } else {
        // Free cam: orbit around track center
        const cx = trackPoints.length > 0
            ? trackPoints.reduce((s, p) => s + p.x, 0) / trackPoints.length
            : 5;
        const cz = trackPoints.length > 0
            ? trackPoints.reduce((s, p) => s + p.z, 0) / trackPoints.length
            : 5;

        cameraAngle += 0.002;
        camera.position.set(
            cx + Math.cos(cameraAngle) * cameraZoom * 0.5,
            cameraZoom,
            cz + Math.sin(cameraAngle) * cameraZoom * 0.5,
        );
        camera.lookAt(cx, 0, cz);
    }
}

// ===========================================
// HUD Updates
// ===========================================
function updateHUD() {
    // Race time
    raceTimeEl.textContent = formatTime(raceState.elapsed_ms || 0);

    // Banner
    if (raceState.phase === 1 && raceState.countdown > 0) {
        bannerEl.textContent = raceState.countdown;
        bannerEl.classList.add('visible');
    } else if (raceState.phase === 1 && raceState.countdown === 0) {
        bannerEl.textContent = 'GO!';
        bannerEl.classList.add('visible');
        bannerEl.style.color = '#0f0';
    } else if (raceState.phase === 3) {
        bannerEl.textContent = 'FINISH';
        bannerEl.classList.add('visible');
        bannerEl.style.color = '#fff';
    } else {
        bannerEl.classList.remove('visible');
    }

    // Leaderboard
    const sorted = Array.from(cars.values()).sort((a, b) => a.position - b.position);
    leaderboardList.innerHTML = sorted.map((car, i) => {
        const posClass = i === 0 ? 'p1' : i === 1 ? 'p2' : i === 2 ? 'p3' : '';
        const colorHex = '#' + (car.color || 0xffffff).toString(16).padStart(6, '0');
        const gap = i === 0
            ? formatTime(car.total_time_ms)
            : (car.total_time_ms > sorted[0].total_time_ms
                ? '+' + formatTimeDelta(car.total_time_ms - sorted[0].total_time_ms)
                : formatTime(car.total_time_ms));
        return `
            <div class="lb-row" style="cursor:pointer" onclick="window.selectCar(${car.car_id})">
                <span class="lb-pos ${posClass}">P${car.position}</span>
                <span class="lb-color" style="background:${colorHex}"></span>
                <span class="lb-name" style="color:${colorHex}">${car.name || 'Car ' + car.car_id}</span>
                <span class="lb-gap">${gap}</span>
                <span class="lb-lap">L${car.lap || 0}/${car.total_laps || 5}</span>
            </div>
        `;
    }).join('');

    // Selected car info
    const sel = cars.get(followCarId);
    if (sel) {
        const colorHex = '#' + (sel.color || 0xffffff).toString(16).padStart(6, '0');
        carNameEl.textContent = sel.name || `Car ${sel.car_id}`;
        carNameEl.style.color = colorHex;
        carSpeedEl.textContent = (sel.speed || 0).toFixed(1);
        carLapEl.textContent = `${sel.lap || 0}/${sel.total_laps || 5}`;
        carBestEl.textContent = sel.best_lap_ms ? formatTime(sel.best_lap_ms) : '--:--';
        carLastEl.textContent = sel.last_lap_ms ? formatTime(sel.last_lap_ms) : '--:--';

        // Active sector
        for (let i = 0; i < 3; i++) {
            sectorEls[i].classList.toggle('active', i === (sel.sector || 0));
        }
    }
}

// ===========================================
// Minimap
// ===========================================
function drawMinimap() {
    const w = minimapCanvas.width;
    const h = minimapCanvas.height;
    minimapCtx.clearRect(0, 0, w, h);

    if (trackPoints.length < 3) return;

    // Find bounds
    let minX = Infinity, maxX = -Infinity, minZ = Infinity, maxZ = -Infinity;
    for (const p of trackPoints) {
        if (p.x < minX) minX = p.x;
        if (p.x > maxX) maxX = p.x;
        if (p.z < minZ) minZ = p.z;
        if (p.z > maxZ) maxZ = p.z;
    }

    const pad = 20;
    const rangeX = maxX - minX || 1;
    const rangeZ = maxZ - minZ || 1;
    const scale = Math.min((w - pad * 2) / rangeX, (h - pad * 2) / rangeZ);

    const toX = (x) => pad + (x - minX) * scale;
    const toY = (z) => pad + (z - minZ) * scale;

    // Track outline
    minimapCtx.beginPath();
    minimapCtx.strokeStyle = '#444';
    minimapCtx.lineWidth = 3;
    for (let i = 0; i <= trackPoints.length; i++) {
        const p = trackPoints[i % trackPoints.length];
        if (i === 0) minimapCtx.moveTo(toX(p.x), toY(p.z));
        else minimapCtx.lineTo(toX(p.x), toY(p.z));
    }
    minimapCtx.stroke();

    // Car dots
    for (const car of cars.values()) {
        const color = '#' + (car.color || 0xffffff).toString(16).padStart(6, '0');
        minimapCtx.beginPath();
        minimapCtx.fillStyle = color;
        minimapCtx.arc(toX(car.x), toY(car.z), 4, 0, Math.PI * 2);
        minimapCtx.fill();

        // Direction indicator
        const dx = Math.cos(car.heading || 0) * 6;
        const dz = Math.sin(car.heading || 0) * 6;
        minimapCtx.beginPath();
        minimapCtx.strokeStyle = color;
        minimapCtx.lineWidth = 1.5;
        minimapCtx.moveTo(toX(car.x), toY(car.z));
        minimapCtx.lineTo(toX(car.x) + dx, toY(car.z) + dz);
        minimapCtx.stroke();
    }
}

// ===========================================
// Trail Rendering
// ===========================================
const trailLines = new Map();

function updateTrails() {
    for (const [carId, trail] of carTrails) {
        if (trail.length < 2) continue;

        const car = cars.get(carId);
        if (!car) continue;

        // Remove old line
        if (trailLines.has(carId)) {
            scene.remove(trailLines.get(carId));
        }

        // Draw new
        const points = trail.map(p => new THREE.Vector3(p.x, 0.01, p.z));
        const geo = new THREE.BufferGeometry().setFromPoints(points);
        const mat = new THREE.LineBasicMaterial({
            color: car.color || 0xffffff,
            transparent: true,
            opacity: 0.3,
        });
        const line = new THREE.Line(geo, mat);
        scene.add(line);
        trailLines.set(carId, line);
    }
}

// ===========================================
// Animation Loop
// ===========================================
function animate() {
    requestAnimationFrame(animate);

    updateCamera();
    updateHUD();
    drawMinimap();

    // Update trails every 30 frames
    if (Math.floor(performance.now() / 500) % 1 === 0) {
        updateTrails();
    }

    renderer.render(scene, camera);
}

// ===========================================
// Input Handlers
// ===========================================
function onKey(e) {
    if (e.key >= '1' && e.key <= '4') {
        followCarId = parseInt(e.key);
        console.log(`[F1T] Following car ${followCarId}`);
    } else if (e.key === '0') {
        followCarId = 0;
        console.log('[F1T] Free camera');
    }
}

function onResize() {
    camera.aspect = window.innerWidth / window.innerHeight;
    camera.updateProjectionMatrix();
    renderer.setSize(window.innerWidth, window.innerHeight);
}

function onWheel(e) {
    cameraZoom += e.deltaY * 0.01;
    cameraZoom = Math.max(3, Math.min(30, cameraZoom));
}

// Global for inline onclick
window.selectCar = (id) => {
    followCarId = id;
};

// ===========================================
// Helpers
// ===========================================
function formatTime(ms) {
    if (!ms && ms !== 0) return '--:--';
    const totalSec = ms / 1000;
    const min = Math.floor(totalSec / 60);
    const sec = totalSec % 60;
    return `${min}:${sec.toFixed(3).padStart(6, '0')}`;
}

function formatTimeDelta(ms) {
    return (ms / 1000).toFixed(3) + 's';
}

// ===========================================
// Go!
// ===========================================
init();
