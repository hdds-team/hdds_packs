/**
 * LiDAR Mapping Viewer
 *
 * Visualizes SLAM (Simultaneous Localization and Mapping):
 * - Occupancy grid map
 * - LiDAR scan rays
 * - Robot position and heading
 * - Environment walls (ground truth)
 */

// Constants
const WORLD_WIDTH = 1000;
const WORLD_HEIGHT = 1000;
const MAP_RESOLUTION = 5;
const MAP_CELLS_X = 200;
const MAP_CELLS_Y = 200;
const LIDAR_RANGE = 300;
const ROBOT_RADIUS = 15;

// Mode names
const MODE_NAMES = {
    0: 'Manual',
    1: 'Explore',
    2: 'Waypoint'
};

// State
let robotPose = null;
let walls = [];
let lidarReadings = [];
let currentScanId = 0;
let occupancyMap = null;
let cellsMapped = 0;
let cellsFree = 0;
let cellsOccupied = 0;

// View settings
let showWalls = true;
let showLidar = true;
let showMap = true;
let followRobot = true;
let viewOffset = { x: 0, y: 0 };
let viewZoom = 0.7;

// Canvas
let canvas, ctx;
let ws = null;
let demoMode = false;

// Demo mode simulator
class DemoSimulator {
    constructor() {
        this.robot = {
            x: 500,
            y: 500,
            heading: 0,
            vx: 0,
            vy: 0,
            omega: 0,
            mode: 1
        };
        this.walls = this.createWalls();
        this.scanId = 0;
    }

    createWalls() {
        const w = [];
        // Outer boundary
        w.push({ x1: 50, y1: 50, x2: 950, y2: 50 });
        w.push({ x1: 950, y1: 50, x2: 950, y2: 950 });
        w.push({ x1: 950, y1: 950, x2: 50, y2: 950 });
        w.push({ x1: 50, y1: 950, x2: 50, y2: 50 });

        // Internal walls
        w.push({ x1: 150, y1: 200, x2: 400, y2: 200 });
        w.push({ x1: 500, y1: 200, x2: 700, y2: 200 });
        w.push({ x1: 200, y1: 350, x2: 450, y2: 350 });
        w.push({ x1: 600, y1: 350, x2: 850, y2: 350 });
        w.push({ x1: 100, y1: 500, x2: 300, y2: 500 });
        w.push({ x1: 400, y1: 500, x2: 600, y2: 500 });
        w.push({ x1: 700, y1: 500, x2: 900, y2: 500 });
        w.push({ x1: 300, y1: 100, x2: 300, y2: 200 });
        w.push({ x1: 600, y1: 100, x2: 600, y2: 200 });
        w.push({ x1: 200, y1: 200, x2: 200, y2: 350 });
        w.push({ x1: 500, y1: 200, x2: 500, y2: 350 });

        return w;
    }

    raycast(ox, oy, angle) {
        const dx = Math.cos(angle);
        const dy = Math.sin(angle);
        let minDist = LIDAR_RANGE;

        for (const wall of this.walls) {
            const dist = this.intersectRay(ox, oy, dx, dy, wall);
            if (dist !== null && dist < minDist) {
                minDist = dist;
            }
        }

        return minDist < LIDAR_RANGE - 1 ? minDist : 0;
    }

    intersectRay(ox, oy, dx, dy, wall) {
        const x1 = wall.x1, y1 = wall.y1;
        const x2 = wall.x2, y2 = wall.y2;
        const x3 = ox, y3 = oy;
        const x4 = ox + dx * LIDAR_RANGE;
        const y4 = oy + dy * LIDAR_RANGE;

        const denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
        if (Math.abs(denom) < 0.0001) return null;

        const t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom;
        const u = -((x1 - x2) * (y1 - y3) - (y1 - y2) * (x1 - x3)) / denom;

        if (t >= 0 && t <= 1 && u >= 0 && u <= 1) {
            const ix = x1 + t * (x2 - x1);
            const iy = y1 + t * (y2 - y1);
            return Math.sqrt((ix - ox) ** 2 + (iy - oy) ** 2);
        }
        return null;
    }

    update(dt) {
        // Simple random walk exploration
        if (Math.random() < 0.02) {
            this.robot.omega = (Math.random() - 0.5) * 2;
        }

        // Check front obstacle
        const frontDist = this.raycast(this.robot.x, this.robot.y, this.robot.heading);
        if (frontDist > 0 && frontDist < 80) {
            this.robot.omega = 1.5 * (Math.random() > 0.5 ? 1 : -1);
            this.robot.vx *= 0.5;
            this.robot.vy *= 0.5;
        } else {
            const speed = 30;
            this.robot.vx = this.robot.vx * 0.95 + Math.cos(this.robot.heading) * speed * 0.05;
            this.robot.vy = this.robot.vy * 0.95 + Math.sin(this.robot.heading) * speed * 0.05;
        }

        this.robot.x += this.robot.vx * dt;
        this.robot.y += this.robot.vy * dt;
        this.robot.heading += this.robot.omega * dt;

        // Clamp
        this.robot.x = Math.max(70, Math.min(930, this.robot.x));
        this.robot.y = Math.max(70, Math.min(930, this.robot.y));

        // Update global state
        robotPose = { ...this.robot };
        walls = this.walls.map((w, i) => ({
            wall_id: i,
            x1: w.x1, y1: w.y1, x2: w.x2, y2: w.y2
        }));

        // Generate LiDAR scan
        this.scanId++;
        const readings = [];
        for (let i = 0; i < 360; i++) {
            const angle = this.robot.heading + (i * Math.PI * 2 / 360);
            const dist = this.raycast(this.robot.x, this.robot.y, angle);
            readings.push([i, dist * 10]); // distance in mm/10
        }
        lidarReadings = readings;
        currentScanId = this.scanId;

        // Update occupancy map
        this.updateMap(readings);
    }

    updateMap(readings) {
        for (const [angleDeg, distMm] of readings) {
            const angle = this.robot.heading + (angleDeg * Math.PI / 180);
            const dist = distMm / 10;

            // Trace ray and mark free
            const step = MAP_RESOLUTION / 2;
            const maxDist = dist > 0 ? dist : LIDAR_RANGE;
            const dx = Math.cos(angle);
            const dy = Math.sin(angle);

            for (let d = 0; d < maxDist - step; d += step) {
                const px = this.robot.x + dx * d;
                const py = this.robot.y + dy * d;
                const cx = Math.floor(px / MAP_RESOLUTION);
                const cy = Math.floor(py / MAP_RESOLUTION);

                if (cx >= 0 && cx < MAP_CELLS_X && cy >= 0 && cy < MAP_CELLS_Y) {
                    if (occupancyMap[cx][cy] === 127) {
                        cellsMapped++;
                        cellsFree++;
                    } else if (occupancyMap[cx][cy] === 255) {
                        cellsOccupied--;
                        cellsFree++;
                    }
                    occupancyMap[cx][cy] = 0;
                }
            }

            // Mark hit as occupied
            if (dist > 0 && dist < LIDAR_RANGE - 1) {
                const hx = this.robot.x + dx * dist;
                const hy = this.robot.y + dy * dist;
                const cx = Math.floor(hx / MAP_RESOLUTION);
                const cy = Math.floor(hy / MAP_RESOLUTION);

                if (cx >= 0 && cx < MAP_CELLS_X && cy >= 0 && cy < MAP_CELLS_Y) {
                    if (occupancyMap[cx][cy] === 127) {
                        cellsMapped++;
                        cellsOccupied++;
                    } else if (occupancyMap[cx][cy] === 0) {
                        cellsFree--;
                        cellsOccupied++;
                    }
                    occupancyMap[cx][cy] = 255;
                }
            }
        }
    }
}

let demoSim = null;

// Initialize
function init() {
    canvas = document.getElementById('map-canvas');
    ctx = canvas.getContext('2d');

    // Initialize occupancy map
    occupancyMap = Array(MAP_CELLS_X).fill(null).map(() =>
        Array(MAP_CELLS_Y).fill(127) // Unknown
    );

    // Check for demo mode
    const params = new URLSearchParams(window.location.search);
    demoMode = params.has('demo');

    if (demoMode) {
        console.log('Running in demo mode (no backend)');
        document.getElementById('connection-status').textContent = 'DEMO MODE';
        document.getElementById('connection-status').className = 'connected';
        demoSim = new DemoSimulator();
    } else {
        connectWebSocket();
    }

    // Setup view controls
    setupControls();

    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);

    // Mouse controls for panning
    setupMouseControls();

    // Start render loop
    let lastTime = performance.now();
    function animate(time) {
        const dt = (time - lastTime) / 1000;
        lastTime = time;

        if (demoMode && demoSim) {
            demoSim.update(dt);
        }

        render();
        updateUI();
        requestAnimationFrame(animate);
    }
    requestAnimationFrame(animate);
}

function resizeCanvas() {
    const container = document.getElementById('map-container');
    const size = Math.min(container.clientWidth - 40, container.clientHeight - 80);
    canvas.width = size;
    canvas.height = size;
}

function setupControls() {
    document.getElementById('btn-walls').addEventListener('click', (e) => {
        showWalls = !showWalls;
        e.target.classList.toggle('active', showWalls);
    });
    document.getElementById('btn-lidar').addEventListener('click', (e) => {
        showLidar = !showLidar;
        e.target.classList.toggle('active', showLidar);
    });
    document.getElementById('btn-map').addEventListener('click', (e) => {
        showMap = !showMap;
        e.target.classList.toggle('active', showMap);
    });
    document.getElementById('btn-follow').addEventListener('click', (e) => {
        followRobot = !followRobot;
        e.target.classList.toggle('active', followRobot);
    });
}

function setupMouseControls() {
    let isDragging = false;
    let lastX, lastY;

    canvas.addEventListener('mousedown', (e) => {
        if (!followRobot) {
            isDragging = true;
            lastX = e.clientX;
            lastY = e.clientY;
        }
    });

    canvas.addEventListener('mousemove', (e) => {
        if (isDragging && !followRobot) {
            viewOffset.x += (e.clientX - lastX) / viewZoom;
            viewOffset.y += (e.clientY - lastY) / viewZoom;
            lastX = e.clientX;
            lastY = e.clientY;
        }
    });

    canvas.addEventListener('mouseup', () => isDragging = false);
    canvas.addEventListener('mouseleave', () => isDragging = false);

    canvas.addEventListener('wheel', (e) => {
        e.preventDefault();
        const zoomFactor = e.deltaY > 0 ? 0.9 : 1.1;
        viewZoom = Math.max(0.2, Math.min(3, viewZoom * zoomFactor));
    });
}

function connectWebSocket() {
    const statusEl = document.getElementById('connection-status');
    const HDDS_WS_URL = `ws://${window.location.hostname || 'localhost'}:9090/ws`;
    const TOPICS = ['RobotPose', 'WallSegment', 'LidarBatch', 'MapBatch'];

    ws = new WebSocket(HDDS_WS_URL);

    ws.onopen = () => {
        console.log('Connected to hdds-ws bridge');
        statusEl.textContent = 'ONLINE';
        statusEl.className = 'connected';

        // Subscribe to all DDS topics via hdds-ws protocol
        for (const topic of TOPICS) {
            ws.send(JSON.stringify({ type: 'subscribe', topic }));
        }
    };

    ws.onclose = () => {
        console.log('Disconnected from server');
        statusEl.textContent = 'OFFLINE';
        statusEl.className = 'disconnected';
        setTimeout(connectWebSocket, 2000);
    };

    ws.onerror = (err) => {
        console.error('WebSocket error:', err);
    };

    ws.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            handleWsMessage(msg);
        } catch (e) {
            console.error('Failed to parse message:', e);
        }
    };
}

function handleWsMessage(msg) {
    switch (msg.type) {
        case 'welcome':
            console.log(`Connected to DDS domain ${msg.domain} (hdds-ws v${msg.version})`);
            break;
        case 'subscribed':
            console.log(`Subscribed to ${msg.topic}`);
            break;
        case 'data':
            handleMessage({ topic: msg.topic, data: fromWire(msg.sample) });
            break;
        case 'error':
            console.error(`DDS error [${msg.code}]: ${msg.message}`);
            break;
    }
}

function handleMessage(msg) {
    const { topic, data } = msg;

    switch (topic) {
        case 'RobotPose':
            robotPose = data;
            break;

        case 'WallSegment':
            walls.push(data);
            break;

        case 'LidarBatch':
            if (data.scan_id !== currentScanId) {
                lidarReadings = [];
                currentScanId = data.scan_id;
            }
            try {
                const readings = JSON.parse(data.readings_json);
                lidarReadings.push(...readings);
            } catch (e) {}
            break;

        case 'MapBatch':
            try {
                const cells = JSON.parse(data.cells_json);
                for (const [cx, cy, occ] of cells) {
                    if (cx < MAP_CELLS_X && cy < MAP_CELLS_Y) {
                        const prev = occupancyMap[cx][cy];
                        occupancyMap[cx][cy] = occ;

                        if (prev === 127) cellsMapped++;
                        if (prev === 0 && occ !== 0) cellsFree--;
                        if (prev === 255 && occ !== 255) cellsOccupied--;
                        if (occ === 0 && prev !== 0) cellsFree++;
                        if (occ === 255 && prev !== 255) cellsOccupied++;
                    }
                }
            } catch (e) {}
            break;
    }
}

function render() {
    const scale = canvas.width / WORLD_WIDTH * viewZoom;

    ctx.fillStyle = '#050508';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    ctx.save();

    // Center and apply view transform
    ctx.translate(canvas.width / 2, canvas.height / 2);

    if (followRobot && robotPose) {
        ctx.translate(-robotPose.x * scale, -robotPose.y * scale);
    } else {
        ctx.translate(viewOffset.x * scale, viewOffset.y * scale);
        ctx.translate(-WORLD_WIDTH / 2 * scale, -WORLD_HEIGHT / 2 * scale);
    }

    // Draw occupancy map
    if (showMap && occupancyMap) {
        drawOccupancyMap(scale);
    }

    // Draw walls (ground truth)
    if (showWalls) {
        drawWalls(scale);
    }

    // Draw LiDAR rays
    if (showLidar && robotPose && lidarReadings.length > 0) {
        drawLidar(scale);
    }

    // Draw robot
    if (robotPose) {
        drawRobot(scale);
    }

    ctx.restore();

    // Draw compass
    drawCompass();
}

function drawOccupancyMap(scale) {
    const cellSize = MAP_RESOLUTION * scale;

    for (let x = 0; x < MAP_CELLS_X; x++) {
        for (let y = 0; y < MAP_CELLS_Y; y++) {
            const occ = occupancyMap[x][y];
            if (occ === 127) continue; // Skip unknown

            if (occ === 0) {
                ctx.fillStyle = '#1a2a1a';
            } else {
                ctx.fillStyle = '#884444';
            }

            ctx.fillRect(
                x * MAP_RESOLUTION * scale,
                y * MAP_RESOLUTION * scale,
                cellSize + 1,
                cellSize + 1
            );
        }
    }
}

function drawWalls(scale) {
    ctx.strokeStyle = '#4488ff';
    ctx.lineWidth = 4 * scale / 0.7;
    ctx.lineCap = 'round';

    for (const wall of walls) {
        ctx.beginPath();
        ctx.moveTo(wall.x1 * scale, wall.y1 * scale);
        ctx.lineTo(wall.x2 * scale, wall.y2 * scale);
        ctx.stroke();
    }
}

function drawLidar(scale) {
    ctx.globalAlpha = 0.3;

    for (const [angleDeg, distMm] of lidarReadings) {
        const angle = robotPose.heading + (angleDeg * Math.PI / 180);
        const dist = distMm / 10; // Convert back to cm

        if (dist <= 0) continue;

        const endX = robotPose.x + Math.cos(angle) * dist;
        const endY = robotPose.y + Math.sin(angle) * dist;

        // Draw ray
        ctx.strokeStyle = '#ff4444';
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(robotPose.x * scale, robotPose.y * scale);
        ctx.lineTo(endX * scale, endY * scale);
        ctx.stroke();

        // Draw hit point
        ctx.fillStyle = '#ff8888';
        ctx.beginPath();
        ctx.arc(endX * scale, endY * scale, 2, 0, Math.PI * 2);
        ctx.fill();
    }

    ctx.globalAlpha = 1;
}

function drawRobot(scale) {
    const x = robotPose.x * scale;
    const y = robotPose.y * scale;
    const radius = ROBOT_RADIUS * scale;

    // Robot body
    ctx.fillStyle = '#ffaa00';
    ctx.beginPath();
    ctx.arc(x, y, radius, 0, Math.PI * 2);
    ctx.fill();

    // Heading indicator
    ctx.strokeStyle = '#ffffff';
    ctx.lineWidth = 3;
    ctx.beginPath();
    ctx.moveTo(x, y);
    ctx.lineTo(
        x + Math.cos(robotPose.heading) * radius * 1.5,
        y + Math.sin(robotPose.heading) * radius * 1.5
    );
    ctx.stroke();

    // Outer ring
    ctx.strokeStyle = '#ffcc44';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(x, y, radius * 1.2, 0, Math.PI * 2);
    ctx.stroke();
}

function drawCompass() {
    const cx = canvas.width - 50;
    const cy = 50;
    const r = 30;

    ctx.save();
    ctx.translate(cx, cy);

    // Circle
    ctx.strokeStyle = '#3a3a45';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(0, 0, r, 0, Math.PI * 2);
    ctx.stroke();

    // N marker
    ctx.fillStyle = '#ff4444';
    ctx.font = '12px sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText('N', 0, -r - 5);

    // Robot heading indicator
    if (robotPose) {
        ctx.rotate(robotPose.heading - Math.PI / 2);
        ctx.fillStyle = '#ffaa00';
        ctx.beginPath();
        ctx.moveTo(0, -r + 5);
        ctx.lineTo(-5, r - 10);
        ctx.lineTo(5, r - 10);
        ctx.closePath();
        ctx.fill();
    }

    ctx.restore();
}

function updateUI() {
    if (robotPose) {
        document.getElementById('position').textContent =
            `(${robotPose.x.toFixed(0)}, ${robotPose.y.toFixed(0)})`;
        document.getElementById('heading').textContent =
            `${(robotPose.heading * 180 / Math.PI).toFixed(1)}°`;
        const speed = Math.sqrt(robotPose.vx ** 2 + robotPose.vy ** 2);
        document.getElementById('speed').textContent = `${speed.toFixed(1)} cm/s`;
        document.getElementById('mode').textContent = MODE_NAMES[robotPose.mode] || '--';
    }

    document.getElementById('scan-id').textContent = currentScanId;
    document.getElementById('readings').textContent = lidarReadings.length;
    document.getElementById('cells-mapped').textContent = cellsMapped;
    document.getElementById('cells-free').textContent = cellsFree;
    document.getElementById('cells-occupied').textContent = cellsOccupied;
}

// Start
init();


// --- wire form of the enum fields -------------------------------------------
// The hdds-ws bridge delivers a field declared as an IDL enum as its label,
// while the code above works on the ordinals of that enum. Incoming samples
// are converted once, here; a field that already carries a number is left
// alone, and so are the viewer's own demo-mode samples, which never pass
// through this function.
const WIRE_ENUMS = {
    mode: ['MANUAL', 'EXPLORE', 'MODE_WAYPOINT'],
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
