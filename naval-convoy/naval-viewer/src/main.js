/**
 * HDDS Naval Convoy - Three.js Viewer
 *
 * 3D visualization of a naval convoy being hunted by submarines.
 * Features ocean shader, ship models, sonar pings, torpedoes, and explosions.
 */

import * as THREE from 'three';

// ═══════════════════════════════════════════════════════════════════════════
// CONFIGURATION
// ═══════════════════════════════════════════════════════════════════════════

const hostname = window.location.hostname || 'localhost';
const HDDS_WS_URL = `ws://${hostname}:9090/ws`;

const TOPICS = {
    SHIP:           'naval/ship',
    CONVOY_STATUS:  'naval/convoy_status',
    SUBMARINE:      'naval/submarine',
    SONAR_PING:     'naval/sonar_ping',
    SONAR_CONTACT:  'naval/sonar_contact',
    TORPEDO:        'naval/torpedo',
    DEPTH_CHARGE:   'naval/depth_charge',
    EXPLOSION:      'naval/explosion',
    COMBAT_EVENT:   'naval/combat_event',
    SEA_STATE:      'naval/sea_state',
};

const SCALE = 100; // 1 nautical mile = 100 units
const RECONNECT_DELAY = 3000;

// ═══════════════════════════════════════════════════════════════════════════
// STATE
// ═══════════════════════════════════════════════════════════════════════════

const state = {
    ships: new Map(),
    submarines: new Map(),
    torpedoes: new Map(),
    depthCharges: new Map(),
    sonarPings: new Map(),
    explosions: [],
    convoyStatus: null,
    events: [],
    connected: false
};

// Three.js objects
let scene, camera, renderer, clock;
let ocean, skybox;
let cameraMode = 'top'; // 'top', 'follow', 'free'

// Object pools
const shipMeshes = new Map();
const subMeshes = new Map();
const torpedoMeshes = new Map();
const pingMeshes = new Map();
const explosionMeshes = [];

// ═══════════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════════

function init() {
    // Scene
    scene = new THREE.Scene();
    scene.fog = new THREE.FogExp2(0x0a1929, 0.0008);

    // Camera
    camera = new THREE.PerspectiveCamera(60, window.innerWidth / window.innerHeight, 1, 10000);
    camera.position.set(0, 800, 0);
    camera.lookAt(0, 0, 0);

    // Renderer
    renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(window.innerWidth, window.innerHeight);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setClearColor(0x0a1929);
    document.getElementById('canvas-container').appendChild(renderer.domElement);

    // Clock
    clock = new THREE.Clock();

    // Lighting
    const ambientLight = new THREE.AmbientLight(0x404060, 0.5);
    scene.add(ambientLight);

    const sunLight = new THREE.DirectionalLight(0xffffff, 1.0);
    sunLight.position.set(100, 200, 50);
    sunLight.castShadow = true;
    scene.add(sunLight);

    // Create ocean
    createOcean();

    // Create skybox/environment
    createSkybox();

    // Event listeners
    window.addEventListener('resize', onResize);
    setupControls();

    // Connect to simulator
    connectWebSocket();

    // Start render loop
    animate();
}

// ═══════════════════════════════════════════════════════════════════════════
// OCEAN SHADER
// ═══════════════════════════════════════════════════════════════════════════

function createOcean() {
    const oceanGeometry = new THREE.PlaneGeometry(5000, 5000, 128, 128);

    const oceanMaterial = new THREE.ShaderMaterial({
        uniforms: {
            uTime: { value: 0 },
            uWaveHeight: { value: 3.0 },
            uWaveFrequency: { value: 0.012 },
            uDeepColor: { value: new THREE.Color(0x0a2540) },
            uShallowColor: { value: new THREE.Color(0x1a6090) },
            uFoamColor: { value: new THREE.Color(0x4488bb) }
        },
        vertexShader: `
            uniform float uTime;
            uniform float uWaveHeight;
            uniform float uWaveFrequency;

            varying vec2 vUv;
            varying float vElevation;

            void main() {
                vUv = uv;

                vec3 pos = position;

                // Multiple wave layers for realistic ocean
                float wave1 = sin(pos.x * uWaveFrequency + uTime * 0.4) *
                              cos(pos.y * uWaveFrequency * 0.7 + uTime * 0.25);
                float wave2 = sin(pos.x * uWaveFrequency * 2.5 + uTime * 0.6) *
                              cos(pos.y * uWaveFrequency * 1.8 + uTime * 0.45) * 0.4;
                float wave3 = sin(pos.x * uWaveFrequency * 0.4 + uTime * 0.15) *
                              cos(pos.y * uWaveFrequency * 0.25 + uTime * 0.1) * 1.5;

                pos.z = (wave1 + wave2 + wave3) * uWaveHeight;
                vElevation = pos.z;

                gl_Position = projectionMatrix * modelViewMatrix * vec4(pos, 1.0);
            }
        `,
        fragmentShader: `
            uniform vec3 uDeepColor;
            uniform vec3 uShallowColor;
            uniform vec3 uFoamColor;
            uniform float uWaveHeight;

            varying vec2 vUv;
            varying float vElevation;

            void main() {
                // Color based on wave height - normalized to 0-1 range
                float maxHeight = uWaveHeight * 2.9;
                float heightFactor = clamp((vElevation / maxHeight + 1.0) * 0.5, 0.0, 1.0);

                // More visible color variation between deep and shallow
                vec3 color = mix(uDeepColor, uShallowColor, heightFactor);

                // Subtle foam/highlights on wave peaks
                float foamFactor = smoothstep(0.75, 0.95, heightFactor);
                color = mix(color, uFoamColor, foamFactor * 0.25);

                // Gentle distance fade at edges
                float dist = length(vUv - 0.5) * 2.0;
                color = mix(color, uDeepColor, smoothstep(0.7, 1.0, dist));

                gl_FragColor = vec4(color, 1.0);
            }
        `,
        transparent: false,
        side: THREE.DoubleSide
    });

    ocean = new THREE.Mesh(oceanGeometry, oceanMaterial);
    ocean.rotation.x = -Math.PI / 2;
    ocean.position.y = 0;
    scene.add(ocean);
}

function createSkybox() {
    // Gradient sky using a large sphere
    const skyGeometry = new THREE.SphereGeometry(4000, 32, 32);
    const skyMaterial = new THREE.ShaderMaterial({
        uniforms: {
            uTopColor: { value: new THREE.Color(0x0a1929) },
            uBottomColor: { value: new THREE.Color(0x1a3a5c) },
            uHorizonColor: { value: new THREE.Color(0x2a5a8c) }
        },
        vertexShader: `
            varying vec3 vWorldPosition;
            void main() {
                vec4 worldPosition = modelMatrix * vec4(position, 1.0);
                vWorldPosition = worldPosition.xyz;
                gl_Position = projectionMatrix * viewMatrix * worldPosition;
            }
        `,
        fragmentShader: `
            uniform vec3 uTopColor;
            uniform vec3 uBottomColor;
            uniform vec3 uHorizonColor;
            varying vec3 vWorldPosition;

            void main() {
                float h = normalize(vWorldPosition).y;
                vec3 color;
                if (h > 0.0) {
                    color = mix(uHorizonColor, uTopColor, h);
                } else {
                    color = mix(uHorizonColor, uBottomColor, -h);
                }
                gl_FragColor = vec4(color, 1.0);
            }
        `,
        side: THREE.BackSide
    });

    skybox = new THREE.Mesh(skyGeometry, skyMaterial);
    scene.add(skybox);
}

// ═══════════════════════════════════════════════════════════════════════════
// ENTITY RENDERING
// ═══════════════════════════════════════════════════════════════════════════

function createShipMesh(ship) {
    const group = new THREE.Group();

    // Hull - simple box for now, could be replaced with GLTF models
    const hullLength = ship.ship_class === 'CARGO' || ship.ship_class === 'TANKER' ? 40 : 25;
    const hullWidth = ship.ship_class === 'CARGO' || ship.ship_class === 'TANKER' ? 12 : 6;
    const hullHeight = 8;

    const hullGeometry = new THREE.BoxGeometry(hullWidth, hullHeight, hullLength);
    const hullMaterial = new THREE.MeshPhongMaterial({
        color: ship.allegiance === 'FRIENDLY' ? 0x4488cc : 0xcc4444,
        flatShading: true
    });
    const hull = new THREE.Mesh(hullGeometry, hullMaterial);
    hull.position.y = 4;
    group.add(hull);

    // Superstructure
    const superGeometry = new THREE.BoxGeometry(hullWidth * 0.6, 12, hullLength * 0.3);
    const superMaterial = new THREE.MeshPhongMaterial({
        color: 0x666666,
        flatShading: true
    });
    const superstructure = new THREE.Mesh(superGeometry, superMaterial);
    superstructure.position.y = 14;
    superstructure.position.z = hullLength * 0.1;
    group.add(superstructure);

    // Name label (using sprite)
    const canvas = document.createElement('canvas');
    canvas.width = 256;
    canvas.height = 64;
    const ctx = canvas.getContext('2d');
    ctx.fillStyle = '#ffffff';
    ctx.font = 'bold 24px Arial';
    ctx.textAlign = 'center';
    ctx.fillText(ship.name, 128, 40);

    const labelTexture = new THREE.CanvasTexture(canvas);
    const labelMaterial = new THREE.SpriteMaterial({ map: labelTexture, transparent: true });
    const label = new THREE.Sprite(labelMaterial);
    label.position.y = 35;
    label.scale.set(60, 15, 1);
    group.add(label);

    // Wake effect
    const wakeGeometry = new THREE.PlaneGeometry(hullWidth * 2, hullLength * 3);
    const wakeMaterial = new THREE.MeshBasicMaterial({
        color: 0xffffff,
        transparent: true,
        opacity: 0.2
    });
    const wake = new THREE.Mesh(wakeGeometry, wakeMaterial);
    wake.rotation.x = -Math.PI / 2;
    wake.position.z = -hullLength * 1.5;
    wake.position.y = 0.5;
    group.add(wake);

    return group;
}

function createSubmarineMesh(sub) {
    const group = new THREE.Group();

    // Hull - elongated cylinder
    const hullGeometry = new THREE.CylinderGeometry(4, 4, 30, 16);
    hullGeometry.rotateZ(Math.PI / 2);
    const hullMaterial = new THREE.MeshPhongMaterial({
        color: 0x333333,
        flatShading: true
    });
    const hull = new THREE.Mesh(hullGeometry, hullMaterial);
    group.add(hull);

    // Conning tower
    const towerGeometry = new THREE.BoxGeometry(3, 6, 8);
    const tower = new THREE.Mesh(towerGeometry, hullMaterial);
    tower.position.y = 5;
    group.add(tower);

    // Periscope (only visible at periscope depth)
    const periscopeGeometry = new THREE.CylinderGeometry(0.3, 0.3, 20, 8);
    const periscope = new THREE.Mesh(periscopeGeometry, hullMaterial);
    periscope.position.y = 13;
    periscope.name = 'periscope';
    group.add(periscope);

    return group;
}

function createTorpedoMesh() {
    const group = new THREE.Group();

    const bodyGeometry = new THREE.CylinderGeometry(0.5, 0.5, 8, 8);
    bodyGeometry.rotateZ(Math.PI / 2);
    const bodyMaterial = new THREE.MeshPhongMaterial({ color: 0x666666 });
    const body = new THREE.Mesh(bodyGeometry, bodyMaterial);
    group.add(body);

    // Wake/trail
    const trailGeometry = new THREE.ConeGeometry(1, 15, 8);
    trailGeometry.rotateZ(-Math.PI / 2);
    const trailMaterial = new THREE.MeshBasicMaterial({
        color: 0xffffff,
        transparent: true,
        opacity: 0.4
    });
    const trail = new THREE.Mesh(trailGeometry, trailMaterial);
    trail.position.x = -10;
    group.add(trail);

    return group;
}

function createSonarPingMesh() {
    // Create a thin ring, not a solid disk
    const geometry = new THREE.RingGeometry(0.95, 1, 64);
    const material = new THREE.MeshBasicMaterial({
        color: 0x00ff88,
        transparent: true,
        opacity: 0.5,
        side: THREE.DoubleSide
    });
    const ring = new THREE.Mesh(geometry, material);
    ring.rotation.x = -Math.PI / 2;
    return ring;
}

function createExplosionMesh() {
    const group = new THREE.Group();

    // Core
    const coreGeometry = new THREE.SphereGeometry(5, 16, 16);
    const coreMaterial = new THREE.MeshBasicMaterial({
        color: 0xff8800,
        transparent: true,
        opacity: 1.0
    });
    const core = new THREE.Mesh(coreGeometry, coreMaterial);
    group.add(core);

    // Outer glow
    const glowGeometry = new THREE.SphereGeometry(10, 16, 16);
    const glowMaterial = new THREE.MeshBasicMaterial({
        color: 0xff4400,
        transparent: true,
        opacity: 0.5
    });
    const glow = new THREE.Mesh(glowGeometry, glowMaterial);
    group.add(glow);

    return group;
}

// ═══════════════════════════════════════════════════════════════════════════
// UPDATE LOOP
// ═══════════════════════════════════════════════════════════════════════════

function updateEntities(delta) {
    // Update ships
    for (const [id, ship] of state.ships) {
        let mesh = shipMeshes.get(id);
        if (!mesh) {
            mesh = createShipMesh(ship);
            shipMeshes.set(id, mesh);
            scene.add(mesh);
        }

        mesh.position.x = ship.x_nm * SCALE;
        mesh.position.z = -ship.y_nm * SCALE;
        mesh.position.y = 2;
        mesh.rotation.y = -ship.heading_deg * Math.PI / 180;

        // Update hull integrity visual (damage)
        if (ship.hull_integrity < 1.0) {
            mesh.children[0].material.color.setHex(
                ship.hull_integrity < 0.5 ? 0x884444 : 0x886666
            );
        }
    }

    // Remove old ship meshes
    for (const [id, mesh] of shipMeshes) {
        if (!state.ships.has(id)) {
            scene.remove(mesh);
            shipMeshes.delete(id);
        }
    }

    // Update submarines
    for (const [id, sub] of state.submarines) {
        let mesh = subMeshes.get(id);
        if (!mesh) {
            mesh = createSubmarineMesh(sub);
            subMeshes.set(id, mesh);
            scene.add(mesh);
        }

        mesh.position.x = sub.x_nm * SCALE;
        mesh.position.z = -sub.y_nm * SCALE;
        mesh.position.y = sub.depth_m * 0.1; // Scale depth for visibility
        mesh.rotation.y = -sub.heading_deg * Math.PI / 180;

        // Show/hide periscope based on state
        const periscope = mesh.getObjectByName('periscope');
        if (periscope) {
            periscope.visible = sub.state === 'PERISCOPE' || sub.state === 'SURFACED';
        }
    }

    // Remove old sub meshes
    for (const [id, mesh] of subMeshes) {
        if (!state.submarines.has(id)) {
            scene.remove(mesh);
            subMeshes.delete(id);
        }
    }

    // Update torpedoes
    for (const [id, torpedo] of state.torpedoes) {
        let mesh = torpedoMeshes.get(id);
        if (!mesh) {
            mesh = createTorpedoMesh();
            torpedoMeshes.set(id, mesh);
            scene.add(mesh);
        }

        mesh.position.x = torpedo.x_nm * SCALE;
        mesh.position.z = -torpedo.y_nm * SCALE;
        mesh.position.y = torpedo.depth_m * 0.1;
        mesh.rotation.y = -torpedo.heading_deg * Math.PI / 180;
    }

    // Remove old torpedo meshes
    for (const [id, mesh] of torpedoMeshes) {
        if (!state.torpedoes.has(id)) {
            scene.remove(mesh);
            torpedoMeshes.delete(id);
        }
    }

    // Update sonar pings
    for (const [id, ping] of state.sonarPings) {
        let mesh = pingMeshes.get(id);
        if (!mesh) {
            mesh = createSonarPingMesh();
            pingMeshes.set(id, mesh);
            scene.add(mesh);
        }

        mesh.position.x = ping.x_nm * SCALE;
        mesh.position.z = -ping.y_nm * SCALE;
        mesh.position.y = 3;

        // Scale the ring - use a reasonable visual size
        const radius = ping.radius_nm * SCALE * 0.3; // Reduce visual scale
        mesh.scale.set(radius, radius, 1);

        // Fade out as ping expands
        const progress = ping.radius_nm / ping.max_range_nm;
        mesh.material.opacity = Math.max(0, 0.7 * (1 - progress * progress));
    }

    // Remove old ping meshes
    for (const [id, mesh] of pingMeshes) {
        if (!state.sonarPings.has(id)) {
            scene.remove(mesh);
            pingMeshes.delete(id);
        }
    }

    // Update explosions
    state.explosions = state.explosions.filter(exp => {
        const age = (Date.now() * 1000 - exp.timestamp_us) / 1000000;
        return age < 2.0;
    });

    // Create/update explosion meshes
    while (explosionMeshes.length < state.explosions.length) {
        const mesh = createExplosionMesh();
        explosionMeshes.push(mesh);
        scene.add(mesh);
    }

    for (let i = 0; i < explosionMeshes.length; i++) {
        const mesh = explosionMeshes[i];
        if (i < state.explosions.length) {
            const exp = state.explosions[i];
            mesh.visible = true;
            mesh.position.x = exp.x_nm * SCALE;
            mesh.position.z = -exp.y_nm * SCALE;
            mesh.position.y = exp.depth_m * 0.1;

            const age = (Date.now() * 1000 - exp.timestamp_us) / 1000000;
            const scale = 1 + age * 3;
            const opacity = Math.max(0, 1 - age / 2);

            mesh.scale.setScalar(scale * exp.magnitude);
            mesh.children.forEach(child => {
                child.material.opacity = opacity;
            });
        } else {
            mesh.visible = false;
        }
    }
}

function updateCamera(delta) {
    if (cameraMode === 'top') {
        camera.position.set(0, 800, 0);
        camera.lookAt(0, 0, 0);
    } else if (cameraMode === 'follow' && state.convoyStatus) {
        const cx = state.convoyStatus.center_x_nm * SCALE;
        const cz = -state.convoyStatus.center_y_nm * SCALE;
        camera.position.set(cx + 200, 300, cz + 200);
        camera.lookAt(cx, 0, cz);
    }
    // 'free' mode: no automatic camera update
}

function updateHUD() {
    if (state.convoyStatus) {
        document.getElementById('ship-count').textContent = state.convoyStatus.ship_count;
        document.getElementById('escort-count').textContent = state.convoyStatus.escort_count;
        document.getElementById('convoy-heading').textContent =
            state.convoyStatus.base_heading_deg.toFixed(0) + '\u00B0';
        document.getElementById('convoy-speed').textContent =
            state.convoyStatus.base_speed_kts.toFixed(1) + ' kts';

        const statusEl = document.getElementById('convoy-status-text');
        statusEl.textContent = state.convoyStatus.status_message;
        statusEl.className = state.convoyStatus.status_message.includes('ATTACK') ||
                             state.convoyStatus.status_message.includes('TORPEDO')
                             ? 'status-alert' : '';
    }

    // Update events log
    const eventsEl = document.getElementById('events-list');
    const recentEvents = state.events.slice(-10).reverse();
    eventsEl.innerHTML = recentEvents.map(e => {
        const time = new Date(e.timestamp_us / 1000).toLocaleTimeString();
        return `<div class="event-item priority-${e.priority}">
            <span class="time">${time}</span>${e.description}
        </div>`;
    }).join('');

    // Update minimap
    updateMinimap();
}

function updateMinimap() {
    const canvas = document.getElementById('minimap-canvas');
    const ctx = canvas.getContext('2d');
    const w = canvas.width;
    const h = canvas.height;
    const mapScale = 15; // pixels per nautical mile

    ctx.fillStyle = '#001830';
    ctx.fillRect(0, 0, w, h);

    // Grid
    ctx.strokeStyle = 'rgba(0, 100, 150, 0.3)';
    ctx.lineWidth = 1;
    for (let i = 0; i <= w; i += 20) {
        ctx.beginPath();
        ctx.moveTo(i, 0);
        ctx.lineTo(i, h);
        ctx.stroke();
        ctx.beginPath();
        ctx.moveTo(0, i);
        ctx.lineTo(w, i);
        ctx.stroke();
    }

    const cx = w / 2;
    const cy = h / 2;

    // Draw ships
    for (const ship of state.ships.values()) {
        const x = cx + ship.x_nm * mapScale;
        const y = cy - ship.y_nm * mapScale;

        ctx.fillStyle = ship.allegiance === 'FRIENDLY' ? '#4488ff' : '#ff4444';
        ctx.beginPath();
        ctx.arc(x, y, 4, 0, Math.PI * 2);
        ctx.fill();

        // Heading indicator
        const hx = x + Math.sin(ship.heading_deg * Math.PI / 180) * 8;
        const hy = y - Math.cos(ship.heading_deg * Math.PI / 180) * 8;
        ctx.strokeStyle = ctx.fillStyle;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(x, y);
        ctx.lineTo(hx, hy);
        ctx.stroke();
    }

    // Draw submarines
    for (const sub of state.submarines.values()) {
        const x = cx + sub.x_nm * mapScale;
        const y = cy - sub.y_nm * mapScale;

        ctx.fillStyle = '#ff6666';
        ctx.strokeStyle = '#ff6666';
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(x, y - 5);
        ctx.lineTo(x + 4, y + 3);
        ctx.lineTo(x - 4, y + 3);
        ctx.closePath();
        ctx.fill();
    }

    // Draw sonar pings
    for (const ping of state.sonarPings.values()) {
        const x = cx + ping.x_nm * mapScale;
        const y = cy - ping.y_nm * mapScale;
        const r = ping.radius_nm * mapScale;

        ctx.strokeStyle = `rgba(0, 255, 136, ${0.6 * (1 - ping.radius_nm / ping.max_range_nm)})`;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(x, y, r, 0, Math.PI * 2);
        ctx.stroke();
    }

    // Draw torpedoes
    for (const torpedo of state.torpedoes.values()) {
        const x = cx + torpedo.x_nm * mapScale;
        const y = cy - torpedo.y_nm * mapScale;

        ctx.fillStyle = '#ff0000';
        ctx.beginPath();
        ctx.arc(x, y, 2, 0, Math.PI * 2);
        ctx.fill();
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WEBSOCKET
// ═══════════════════════════════════════════════════════════════════════════

let ws = null;

function connectWebSocket() {
    ws = new WebSocket(HDDS_WS_URL);

    ws.onopen = () => {
        console.log('Connected to HDDS bridge');
        state.connected = true;
        updateConnectionStatus(true);

        // Subscribe to all naval topics
        for (const topic of Object.values(TOPICS)) {
            ws.send(JSON.stringify({ type: 'subscribe', topic }));
        }

        document.getElementById('loading').classList.add('hidden');
    };

    ws.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            handleWsMessage(msg);
        } catch (e) {
            console.error('Failed to parse message:', e);
        }
    };

    ws.onclose = () => {
        console.log('Disconnected from HDDS bridge');
        state.connected = false;
        updateConnectionStatus(false);
        setTimeout(connectWebSocket, RECONNECT_DELAY);
    };

    ws.onerror = (error) => {
        console.error('HDDS WebSocket error:', error);
    };
}

function handleWsMessage(msg) {
    switch (msg.type) {
        case 'welcome':
            console.log(`Connected to DDS domain ${msg.domain}`);
            break;

        case 'subscribed':
            console.log(`Subscribed to ${msg.topic}`);
            break;

        case 'data':
            handleDdsData(msg.topic, msg.sample);
            break;

        case 'error':
            console.error(`DDS error: ${msg.message}`);
            break;
    }
}

function handleDdsData(topic, sample) {
    switch (topic) {
        case TOPICS.SHIP:
            state.ships.set(sample.ship_id, sample);
            break;
        case TOPICS.SUBMARINE:
            state.submarines.set(sample.sub_id, sample);
            break;
        case TOPICS.TORPEDO:
            state.torpedoes.set(sample.torpedo_id, sample);
            break;
        case TOPICS.DEPTH_CHARGE:
            state.depthCharges.set(sample.charge_id, sample);
            break;
        case TOPICS.SONAR_PING:
            state.sonarPings.set(sample.ping_id, sample);
            break;
        case TOPICS.SONAR_CONTACT:
            // Sonar contacts could be displayed as markers on minimap
            break;
        case TOPICS.EXPLOSION:
            if (!state.explosions.find(e => e.explosion_id === sample.explosion_id)) {
                state.explosions.push(sample);
            }
            break;
        case TOPICS.COMBAT_EVENT:
            if (!state.events.find(e => e.event_id === sample.event_id)) {
                state.events.push(sample);
                if (state.events.length > 50) {
                    state.events = state.events.slice(-50);
                }
            }
            break;
        case TOPICS.CONVOY_STATUS:
            state.convoyStatus = sample;
            break;
        case TOPICS.SEA_STATE:
            // Could update ocean shader parameters
            break;
    }
}

function updateConnectionStatus(connected) {
    const el = document.getElementById('connection-status');
    el.innerHTML = connected
        ? '<span class="status-connected">Connected</span>'
        : '<span class="status-disconnected">Disconnected - Reconnecting...</span>';
}

// ═══════════════════════════════════════════════════════════════════════════
// CONTROLS
// ═══════════════════════════════════════════════════════════════════════════

function setupControls() {
    document.getElementById('btn-top').addEventListener('click', () => {
        cameraMode = 'top';
        updateControlButtons();
    });

    document.getElementById('btn-follow').addEventListener('click', () => {
        cameraMode = 'follow';
        updateControlButtons();
    });

    document.getElementById('btn-free').addEventListener('click', () => {
        cameraMode = 'free';
        updateControlButtons();
        // Enable orbit controls for free camera
    });

    // Keyboard controls
    document.addEventListener('keydown', (e) => {
        if (cameraMode === 'free') {
            const speed = 20;
            switch (e.key) {
                case 'w': camera.position.z -= speed; break;
                case 's': camera.position.z += speed; break;
                case 'a': camera.position.x -= speed; break;
                case 'd': camera.position.x += speed; break;
                case 'q': camera.position.y += speed; break;
                case 'e': camera.position.y -= speed; break;
            }
        }
    });
}

function updateControlButtons() {
    document.querySelectorAll('#controls button').forEach(btn => {
        btn.classList.remove('active');
    });
    document.getElementById(`btn-${cameraMode}`).classList.add('active');
}

function onResize() {
    camera.aspect = window.innerWidth / window.innerHeight;
    camera.updateProjectionMatrix();
    renderer.setSize(window.innerWidth, window.innerHeight);
}

// ═══════════════════════════════════════════════════════════════════════════
// ANIMATION LOOP
// ═══════════════════════════════════════════════════════════════════════════

function animate() {
    requestAnimationFrame(animate);

    const delta = clock.getDelta();

    // Update ocean shader
    if (ocean) {
        ocean.material.uniforms.uTime.value += delta;
    }

    // Update entities
    updateEntities(delta);

    // Update camera
    updateCamera(delta);

    // Update HUD
    updateHUD();

    // Render
    renderer.render(scene, camera);
}

// Start
init();
