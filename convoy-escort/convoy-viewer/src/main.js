/**
 * Convoy Escort Viewer
 *
 * Three.js visualization connected to HDDS via hdds-ws bridge
 */

import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

// Configuration
const HDDS_WS_URL = 'ws://localhost:9090/ws';
const TOPICS = {
    GPS: 'convoy/vehicle/gps',
    STATE: 'convoy/vehicle/state',
    SUMMARY: 'convoy/summary',
    ALERT: 'convoy/alert'
};

// Debug: log first few messages to console
let debugCounter = 0;

// CDR decoder for VehicleGPS when dynamic decoding fails
function decodeVehicleGPS(base64) {
    try {
        // Decode base64
        const binary = atob(base64);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
        }
        const view = new DataView(bytes.buffer);

        // No CDR encapsulation header - data starts directly
        let offset = 0;

        // string<32> vehicle_id (4-byte length + chars, no null terminator in length)
        const idLen = view.getUint32(offset, true);
        offset += 4;
        let vehicle_id = '';
        for (let i = 0; i < idLen && bytes[offset + i] !== 0; i++) {
            vehicle_id += String.fromCharCode(bytes[offset + i]);
        }
        offset += idLen;
        // Align to 4 bytes
        offset = (offset + 3) & ~3;

        // VehicleRole role (enum = 4 bytes)
        const role = view.getUint32(offset, true);
        offset += 4;

        // Align to 8 bytes for double
        offset = (offset + 7) & ~7;

        // double latitude
        const latitude = view.getFloat64(offset, true);
        offset += 8;

        // double longitude
        const longitude = view.getFloat64(offset, true);
        offset += 8;

        // float altitude_m
        const altitude_m = view.getFloat32(offset, true);
        offset += 4;

        // float heading_deg
        const heading_deg = view.getFloat32(offset, true);
        offset += 4;

        // float speed_kmh
        const speed_kmh = view.getFloat32(offset, true);
        offset += 4;

        // float course_deg
        const course_deg = view.getFloat32(offset, true);
        offset += 4;

        // Align to 8 bytes for uint64
        offset = (offset + 7) & ~7;

        // uint64 timestamp_ms
        const timestamp_ms = Number(view.getBigUint64(offset, true));

        const result = {
            vehicle_id,
            role,
            latitude,
            longitude,
            altitude_m,
            heading_deg,
            speed_kmh,
            course_deg,
            timestamp_ms
        };

        // Debug first decode
        if (debugCounter < 3) {
            console.log('CDR decoded:', result);
        }

        return result;
    } catch (e) {
        console.error('CDR decode error:', e);
        return null;
    }
}

// Coordinate conversion (lat/lon to scene coordinates)
// Center around Paris
const CENTER_LAT = 48.8566;
const CENTER_LON = 2.3522;
const SCALE = 100000; // 1 degree ≈ 100km, so 100000 units per degree

function latLonToXZ(lat, lon) {
    return {
        x: (lon - CENTER_LON) * SCALE * Math.cos(CENTER_LAT * Math.PI / 180),
        z: -(lat - CENTER_LAT) * SCALE
    };
}

// State
const state = {
    vehicles: new Map(),
    summary: null,
    alerts: []
};

// Three.js globals
let scene, camera, renderer, controls;
let terrain, road, terrainGroup, vehicleMeshes = new Map();
let gltfLoader;
let vehicleModel = null;

// WebSocket
let ws = null;
let reconnectTimer = null;

// ==================== THREE.JS SETUP ====================

function initScene() {
    // Scene
    scene = new THREE.Scene();
    scene.background = new THREE.Color(0x87ceeb); // Sky blue
    scene.fog = new THREE.Fog(0x87ceeb, 500, 2000);

    // Camera
    camera = new THREE.PerspectiveCamera(60, window.innerWidth / window.innerHeight, 1, 5000);
    camera.position.set(5, 40, 80); // Behind and above the convoy

    // Renderer
    renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(window.innerWidth, window.innerHeight);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    document.getElementById('canvas-container').appendChild(renderer.domElement);

    // Controls
    controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.05;
    controls.maxPolarAngle = Math.PI / 2 - 0.1;
    controls.minDistance = 50;
    controls.maxDistance = 1000;

    // Lights
    const ambientLight = new THREE.AmbientLight(0xffffff, 0.4);
    scene.add(ambientLight);

    const sunLight = new THREE.DirectionalLight(0xffffff, 1.0);
    sunLight.position.set(100, 200, 100);
    sunLight.castShadow = true;
    sunLight.shadow.mapSize.width = 2048;
    sunLight.shadow.mapSize.height = 2048;
    sunLight.shadow.camera.near = 10;
    sunLight.shadow.camera.far = 1000;
    sunLight.shadow.camera.left = -500;
    sunLight.shadow.camera.right = 500;
    sunLight.shadow.camera.top = 500;
    sunLight.shadow.camera.bottom = -500;
    scene.add(sunLight);

    // Create terrain
    createTerrain();

    // Load vehicle model
    gltfLoader = new GLTFLoader();
    gltfLoader.load('/assets/models/escort-vehicle.glb', (gltf) => {
        vehicleModel = gltf.scene;
        vehicleModel.traverse((node) => {
            if (node.isMesh) {
                node.castShadow = true;
                node.receiveShadow = true;
            }
        });
        console.log('Vehicle model loaded');
    }, undefined, (error) => {
        // Leave the model unset: createVehicle then builds its own shaped
        // group, which is the right size. A box substituted here would be
        // scaled a second time by the caller and tower over the convoy.
        console.error('Error loading vehicle model:', error);
    });

    // Window resize
    window.addEventListener('resize', onWindowResize);
}

function createTerrain() {
    // Terrain group follows the convoy so the road is always underfoot
    terrainGroup = new THREE.Group();
    scene.add(terrainGroup);

    // Procedural terrain with simple hills
    const size = 2000;
    const segments = 100;

    const geometry = new THREE.PlaneGeometry(size, size, segments, segments);
    geometry.rotateX(-Math.PI / 2);

    // Add gentle height variation, flat near the road (x=0)
    const vertices = geometry.attributes.position.array;
    for (let i = 0; i < vertices.length; i += 3) {
        const x = vertices[i];
        const z = vertices[i + 2];
        // Gentle rolling hills
        const baseHeight =
            Math.sin(x * 0.01) * 2 +
            Math.cos(z * 0.01) * 1.5 +
            Math.sin(x * 0.005 + z * 0.005) * 3;
        // Flatten near road (x=0): fade terrain to 0 within 30 units of road center
        const roadDist = Math.abs(x);
        const roadFade = Math.min(1, Math.max(0, (roadDist - 15) / 30));
        vertices[i + 1] = baseHeight * roadFade;
    }

    geometry.computeVertexNormals();

    // Grass-like material
    const material = new THREE.MeshStandardMaterial({
        color: 0x3d6b3d,
        roughness: 0.9,
        metalness: 0.0,
        flatShading: false
    });

    terrain = new THREE.Mesh(geometry, material);
    terrain.receiveShadow = true;
    terrainGroup.add(terrain);

    // Add a road (simple dark strip)
    const roadGeometry = new THREE.PlaneGeometry(20, size);
    roadGeometry.rotateX(-Math.PI / 2);
    const roadMaterial = new THREE.MeshStandardMaterial({
        color: 0x333333,
        roughness: 0.8
    });
    road = new THREE.Mesh(roadGeometry, roadMaterial);
    road.position.y = 0.5;
    road.receiveShadow = true;
    terrainGroup.add(road);

    // Road markings
    const markingGeometry = new THREE.PlaneGeometry(0.5, 10);
    markingGeometry.rotateX(-Math.PI / 2);
    const markingMaterial = new THREE.MeshBasicMaterial({ color: 0xffffff });
    for (let z = -size/2; z < size/2; z += 30) {
        const marking = new THREE.Mesh(markingGeometry, markingMaterial);
        marking.position.set(0, 0.6, z);
        terrainGroup.add(marking);
    }
}

function createVehicleMesh(vehicleId, role) {
    let mesh;

    // Color by role
    const roleColors = {
        'LEAD': 0x60a5fa,   // Blue
        'VIP': 0xfbbf24,    // Gold
        'ESCORT': 0xa78bfa, // Purple
        'REAR': 0x4ade80    // Green
    };
    const color = roleColors[role] || 0x888888;

    if (vehicleModel) {
        mesh = vehicleModel.clone();
        mesh.scale.set(2, 2, 2);
    } else {
        // Fallback box (vehicle-shaped)
        const group = new THREE.Group();
        const body = new THREE.Mesh(
            new THREE.BoxGeometry(3, 1.5, 6),
            new THREE.MeshStandardMaterial({ color })
        );
        body.position.y = 0.75;
        body.castShadow = true;
        group.add(body);
        const cabin = new THREE.Mesh(
            new THREE.BoxGeometry(2.5, 1, 3),
            new THREE.MeshStandardMaterial({ color: 0x222222 })
        );
        cabin.position.y = 1.75;
        cabin.position.z = -0.5;
        cabin.castShadow = true;
        group.add(cabin);
        mesh = group;
    }

    mesh.castShadow = true;
    mesh.receiveShadow = true;

    // Add label showing ROLE name (not raw vehicle_id which can be garbled)
    const canvas = document.createElement('canvas');
    canvas.width = 256;
    canvas.height = 64;
    const ctx = canvas.getContext('2d');

    ctx.fillStyle = 'rgba(0,0,0,0.7)';
    ctx.fillRect(0, 0, 256, 64);

    ctx.fillStyle = role === 'VIP' ? '#fbbf24' :
                    role === 'LEAD' ? '#60a5fa' :
                    role === 'REAR' ? '#4ade80' : '#a78bfa';
    ctx.font = 'bold 32px Arial';
    ctx.textAlign = 'center';
    ctx.fillText(role, 128, 42);

    const labelTexture = new THREE.CanvasTexture(canvas);
    const labelMaterial = new THREE.SpriteMaterial({ map: labelTexture });
    const label = new THREE.Sprite(labelMaterial);
    label.scale.set(3, 0.8, 1);
    label.position.y = 3;
    mesh.add(label);

    // Store role for updates
    mesh.userData.role = role;

    scene.add(mesh);
    return mesh;
}

function onWindowResize() {
    camera.aspect = window.innerWidth / window.innerHeight;
    camera.updateProjectionMatrix();
    renderer.setSize(window.innerWidth, window.innerHeight);
}

// ==================== WEBSOCKET ====================

function connectWebSocket() {
    updateConnectionStatus('connecting');

    ws = new WebSocket(HDDS_WS_URL);

    ws.onopen = () => {
        console.log('Connected to HDDS bridge');
        updateConnectionStatus('connected');

        // Subscribe to topics
        ws.send(JSON.stringify({ type: 'subscribe', topic: TOPICS.GPS }));
        ws.send(JSON.stringify({ type: 'subscribe', topic: TOPICS.STATE }));
        ws.send(JSON.stringify({ type: 'subscribe', topic: TOPICS.SUMMARY }));
        ws.send(JSON.stringify({ type: 'subscribe', topic: TOPICS.ALERT }));

        // Hide loading screen
        document.getElementById('loading').classList.add('hidden');
    };

    ws.onmessage = (event) => {
        const msg = JSON.parse(event.data);
        handleMessage(msg);
    };

    ws.onclose = () => {
        console.log('Disconnected from HDDS bridge');
        updateConnectionStatus('disconnected');
        scheduleReconnect();
    };

    ws.onerror = (error) => {
        console.error('WebSocket error:', error);
        updateConnectionStatus('disconnected');
    };
}

function scheduleReconnect() {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = setTimeout(() => {
        console.log('Reconnecting...');
        connectWebSocket();
    }, 3000);
}

function handleMessage(msg) {
    switch (msg.type) {
        case 'welcome':
            console.log(`Connected to DDS domain ${msg.domain}`);
            break;

        case 'data':
            handleData(msg.topic, msg.sample);
            break;

        case 'subscribed':
            console.log(`Subscribed to ${msg.topic}`);
            break;

        case 'error':
            console.error(`DDS error: ${msg.message}`);
            break;
    }
}

function handleData(topic, sample) {
    // Debug: log first messages to see structure
    if (debugCounter < 5) {
        console.log(`[${topic}] Sample:`, JSON.stringify(sample, null, 2));
        debugCounter++;
    }

    switch (topic) {
        case TOPICS.GPS:
            updateVehiclePosition(sample);
            break;

        case TOPICS.STATE:
            updateVehicleState(sample);
            break;

        case TOPICS.SUMMARY:
            updateConvoySummary(sample);
            break;

        case TOPICS.ALERT:
            addAlert(sample);
            break;
    }
}

// ==================== DATA HANDLERS ====================

function updateVehiclePosition(gps) {
    // Handle both dynamic decoded format and fallback format
    // Dynamic: { vehicle_id, role, latitude, longitude, ... }
    // Fallback: { data: "LEAD-1", _raw: "..." }

    // If we have raw CDR data, decode it
    if (gps._raw && gps.latitude === undefined) {
        const decoded = decodeVehicleGPS(gps._raw);
        if (decoded) {
            gps = decoded;
        }
    }

    let id = gps.vehicle_id || gps.data;
    if (!id) return; // Skip if no valid vehicle_id

    // Get role - handle both integer (CDR) and string (XTypes) formats
    let role = gps.role;
    if (typeof role === 'string') {
        role = { LEAD: 0, VIP: 1, ESCORT: 2, REAR: 3 }[role] ?? 2;
    } else if (role === undefined || role === null) {
        // Fallback: deduce role from vehicle ID
        if (id.includes('LEAD')) role = 0;
        else if (id.includes('VIP')) role = 1;
        else if (id.includes('ESCORT')) role = 2;
        else if (id.includes('REAR')) role = 3;
        else role = 2; // Default ESCORT
    }

    // Get coordinates
    let lat = gps.latitude;
    let lon = gps.longitude;
    let heading = gps.heading_deg || 90;
    let speed = gps.speed_kmh || 60;

    // Last resort fallback if still no coordinates
    if (lat === undefined) {
        lat = CENTER_LAT;
        lon = CENTER_LON;
    }

    // Store vehicle data
    const vehicleData = {
        ...state.vehicles.get(id),
        ...gps,
        role,
        latitude: lat,
        longitude: lon,
        heading_deg: heading,
        speed_kmh: speed
    };
    state.vehicles.set(id, vehicleData);

    // Update or create 3D mesh
    let mesh = vehicleMeshes.get(id);
    if (!mesh) {
        const roleStr = ['LEAD', 'VIP', 'ESCORT', 'REAR'][role] || 'ESCORT';
        mesh = createVehicleMesh(id, roleStr);
        vehicleMeshes.set(id, mesh);
    }

    // Convert lat/lon to scene coordinates
    const pos = latLonToXZ(lat, lon);
    // Keep vehicles on the road (X near 0) with small lateral offset based on role
    const lateralOffset = (role === 1) ? 0 :      // VIP in center
                          (role === 0) ? -2 :     // LEAD slightly left
                          (role === 2) ? 3 :      // ESCORT slightly right
                          0;                      // REAR in center
    mesh.position.x = lateralOffset;
    mesh.position.z = pos.z;
    mesh.position.y = getTerrainHeight(0, pos.z) + 1.5;

    // Rotate to heading
    // Heading 0 = North = -Z in scene. Model default faces +Z after PI flip.
    // So heading 0 needs Y rotation of PI (turn 180 to face -Z).
    // General: Y rotation = PI + heading_in_radians
    const headingRad = heading * Math.PI / 180;
    mesh.rotation.set(Math.PI, Math.PI/2 + headingRad, 0);

    // Update HUD
    updateVehicleList();
    updateMinimap();
}

function updateVehicleState(state_data) {
    const id = state_data.vehicle_id;
    if (!id) return; // Skip if no valid vehicle_id
    const current = state.vehicles.get(id) || {};
    state.vehicles.set(id, { ...current, ...state_data });
    updateVehicleList();
}

function updateConvoySummary(summary) {
    state.summary = summary;

    // Update HUD (with null guards for XTypes decode)
    document.getElementById('mission-name').textContent = summary.convoy_name || 'VIP Escort';
    const total = summary.total_vehicles ?? state.vehicles.size;
    const operational = summary.operational_count ?? state.vehicles.size;
    document.getElementById('vehicle-count').textContent = `${operational}/${total}`;
    const speed = typeof summary.avg_speed_kmh === 'number' ? summary.avg_speed_kmh.toFixed(1) : '0';
    document.getElementById('avg-speed').textContent = `${speed} km/h`;
    const progress = typeof summary.progress_percent === 'number' ? summary.progress_percent.toFixed(1) : '0';
    document.getElementById('progress').textContent = `${progress}%`;

    const threatEl = document.getElementById('threat-level');
    const threats = ['GREEN', 'YELLOW', 'ORANGE', 'RED'];
    let threatIdx = summary.current_threat;
    if (typeof threatIdx === 'string') threatIdx = threats.indexOf(threatIdx);
    const threat = threats[threatIdx] || 'GREEN';
    threatEl.textContent = threat;
    threatEl.className = `threat-${threat.toLowerCase()}`;

    // Center camera on convoy (road-relative: X=0, only Z moves)
    if (summary.center_lat && summary.center_lon) {
        const pos = latLonToXZ(summary.center_lat, summary.center_lon);
        controls.target.set(0, 0, pos.z);
        // Move terrain group to follow the convoy so road stays underfoot
        if (terrainGroup) {
            terrainGroup.position.z = pos.z;
        }
    }
}

function addAlert(alert) {
    state.alerts.unshift(alert);
    if (state.alerts.length > 5) state.alerts.pop();

    const container = document.getElementById('alerts-container');
    container.innerHTML = '';

    for (const a of state.alerts) {
        const types = ['IED', 'AMBUSH', 'ROADBLOCK', 'VEHICLE_BREAKDOWN', 'CIVILIAN_HAZARD', 'ALL_CLEAR'];
        const typeLabels = ['IED', 'AMBUSH', 'ROADBLOCK', 'BREAKDOWN', 'CIVILIAN', 'ALL CLEAR'];
        let alertIdx = a.alert_type;
        if (typeof alertIdx === 'string') alertIdx = types.indexOf(alertIdx);
        const typeStr = typeLabels[alertIdx] || 'ALERT';
        const threats = ['GREEN', 'YELLOW', 'ORANGE', 'RED'];
        let threatIdx = a.threat_level;
        if (typeof threatIdx === 'string') threatIdx = threats.indexOf(threatIdx);
        const threatStr = threats[threatIdx] || 'YELLOW';

        const div = document.createElement('div');
        div.className = `alert-item threat-${threatStr.toLowerCase()}`;
        div.innerHTML = `<strong>${typeStr}</strong>: ${a.description || 'Unknown'}`;
        container.appendChild(div);
    }
}

function updateVehicleList() {
    const container = document.getElementById('vehicles-container');
    container.innerHTML = '';

    for (const [id, v] of state.vehicles) {
        const roles = ['LEAD', 'VIP', 'ESCORT', 'REAR'];
        let roleIdx = v.role;
        if (typeof roleIdx === 'string') roleIdx = roles.indexOf(roleIdx);
        const roleStr = roles[roleIdx] || 'ESCORT';
        let statusIdx = v.status;
        if (typeof statusIdx === 'string') statusIdx = ['OPERATIONAL', 'DAMAGED', 'DISABLED', 'OFFLINE'].indexOf(statusIdx);
        const statusStr = ['OK', 'DMG', 'DIS', 'OFF'][statusIdx] || 'OK';
        const speed = typeof v.speed_kmh === 'number' ? v.speed_kmh.toFixed(0) : '0';

        const div = document.createElement('div');
        div.className = 'vehicle-item';
        div.innerHTML = `
            <span class="role-${roleStr.toLowerCase()}">${roleStr}</span>
            <span>${speed} km/h</span>
            <span>${statusStr}</span>
        `;
        container.appendChild(div);
    }
}

function updateMinimap() {
    const canvas = document.getElementById('minimap-canvas');
    const ctx = canvas.getContext('2d');

    ctx.fillStyle = '#0d1117';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // Draw grid
    ctx.strokeStyle = '#1e3a2f';
    ctx.lineWidth = 1;
    for (let i = 0; i <= 4; i++) {
        const y = i * canvas.height / 4;
        const x = i * canvas.width / 4;
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(canvas.width, y);
        ctx.stroke();
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, canvas.height);
        ctx.stroke();
    }

    // Draw road line (vertical center)
    ctx.strokeStyle = '#333333';
    ctx.lineWidth = 8;
    ctx.beginPath();
    ctx.moveTo(canvas.width / 2, 0);
    ctx.lineTo(canvas.width / 2, canvas.height);
    ctx.stroke();

    // Draw vehicles using road-relative positions (matching 3D view)
    const cx = canvas.width / 2;
    const cy = canvas.height / 2;

    // Compute Z bounding box for auto-scale (only Z matters, X is lateral offset)
    let minZ = Infinity, maxZ = -Infinity;
    for (const [, v] of state.vehicles) {
        if (v.latitude === undefined) continue;
        const p = latLonToXZ(v.latitude, v.longitude);
        minZ = Math.min(minZ, p.z); maxZ = Math.max(maxZ, p.z);
    }
    const rangeZ = Math.max(maxZ - minZ, 100);
    const scale = (canvas.height - 30) / rangeZ;
    const centerZ = (minZ + maxZ) / 2;

    // Role lateral offsets (same as 3D view)
    const lateralOffsets = { 0: -3, 1: 0, 2: 3, 3: 0 };

    for (const [id, v] of state.vehicles) {
        if (v.latitude === undefined) continue;

        const pos = latLonToXZ(v.latitude, v.longitude);
        const lateral = lateralOffsets[v.role] || 0;
        const mx = cx + lateral * 2; // Scale lateral for visibility on minimap
        const my = cy + (pos.z - centerZ) * scale;

        // Color by role
        const colors = ['#60a5fa', '#fbbf24', '#a78bfa', '#4ade80'];
        ctx.fillStyle = colors[v.role] || '#ffffff';

        ctx.beginPath();
        ctx.arc(mx, my, 5, 0, Math.PI * 2);
        ctx.fill();

        // Direction indicator (heading 0=North=up on minimap)
        const angle = -(v.heading_deg || 0) * Math.PI / 180;
        ctx.strokeStyle = ctx.fillStyle;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(mx, my);
        ctx.lineTo(mx + Math.sin(angle) * 10, my - Math.cos(angle) * 10);
        ctx.stroke();

        // Label (role name)
        const roleNames = ['LEAD', 'VIP', 'ESC', 'REAR'];
        ctx.font = '9px monospace';
        ctx.fillText(roleNames[v.role] || '?', mx + 8, my + 3);
    }
}

function getTerrainHeight(x, z) {
    // Convert world Z to terrain-local Z (terrain moves with convoy)
    const localZ = terrainGroup ? z - terrainGroup.position.z : z;
    // Match the procedural terrain height calculation
    const baseHeight =
        Math.sin(x * 0.01) * 2 +
        Math.cos(localZ * 0.01) * 1.5 +
        Math.sin(x * 0.005 + localZ * 0.005) * 3;
    const roadDist = Math.abs(x);
    const roadFade = Math.min(1, Math.max(0, (roadDist - 15) / 30));
    return baseHeight * roadFade;
}

function updateConnectionStatus(status) {
    const el = document.getElementById('connection-status').querySelector('span');
    el.className = `status-${status}`;
    el.textContent = status === 'connected' ? 'Connected to HDDS' :
                     status === 'connecting' ? 'Connecting...' : 'Disconnected';
}

// ==================== ANIMATION LOOP ====================

function animate() {
    requestAnimationFrame(animate);
    controls.update();
    renderer.render(scene, camera);
}

// ==================== INIT ====================

initScene();
connectWebSocket();
animate();

console.log('Convoy Escort Viewer initialized');
console.log('Waiting for HDDS data on topics:', Object.values(TOPICS));
