/**
 * Robot Swarm Viewer
 *
 * Canvas 2D visualization of robot swarm behaviors:
 * - Top-down view of arena
 * - Robot positions with heading indicators
 * - Swarm centroid and spread visualization
 * - Obstacles and waypoints
 * - Real-time status panel
 */

// Constants
const ARENA_WIDTH = 500;
const ARENA_HEIGHT = 500;
const ROBOT_RADIUS = 8;

// Mode and formation names
const MODE_NAMES = {
    0: 'Idle',
    1: 'Flock',
    2: 'Formation',
    3: 'Follow Leader',
    4: 'Scatter',
    5: 'Gather',
    6: 'Patrol',
    7: 'Search'
};

const FORMATION_NAMES = {
    0: 'Line',
    1: 'Circle',
    2: 'Wedge',
    3: 'Grid',
    4: 'Diamond'
};

// State
const robots = new Map();
const obstacles = new Map();
const waypoints = new Map();
let swarmState = null;
let ws = null;
let demoMode = false;
let canvas, ctx;
let scale = 1;

// Demo mode data generators
class DemoSimulator {
    constructor() {
        this.robots = [];
        this.mode = 1; // Flock
        this.formation = 1;
        this.modeTimer = 0;
        this.initRobots();
    }

    initRobots() {
        const names = ['ALPHA', 'BRAVO', 'CHARLIE', 'DELTA', 'ECHO', 'FOXTROT',
                       'GOLF', 'HOTEL', 'INDIA', 'JULIET', 'KILO', 'LIMA'];
        const colors = [0xFF6B6B, 0x4ECDC4, 0x45B7D1, 0xF7DC6F, 0xBB8FCE, 0x82E0AA,
                        0xF8B500, 0xE74C3C, 0x3498DB, 0x1ABC9C, 0x9B59B6, 0xE67E22];

        for (let i = 0; i < 12; i++) {
            this.robots.push({
                robot_id: i,
                name: names[i],
                color: colors[i],
                x: 100 + Math.random() * 300,
                y: 100 + Math.random() * 300,
                heading: Math.random() * Math.PI * 2,
                vx: 0,
                vy: 0,
                omega: 0,
                state: 1,
                battery: 70 + Math.floor(Math.random() * 30),
                front_dist: 50,
                left_dist: 50,
                right_dist: 50,
                is_leader: i === 0 ? 1 : 0,
                group_id: 0,
                timestamp_us: Date.now() * 1000
            });
        }
    }

    update(dt) {
        this.modeTimer += dt;
        if (this.modeTimer > 10) {
            this.modeTimer = 0;
            this.mode = (this.mode % 6) + 1;
            if (this.mode === 2) {
                this.formation = (this.formation + 1) % 5;
            }
        }

        const centroidX = this.robots.reduce((s, r) => s + r.x, 0) / this.robots.length;
        const centroidY = this.robots.reduce((s, r) => s + r.y, 0) / this.robots.length;

        for (const robot of this.robots) {
            // Simple boids-like behavior
            let vx = 0, vy = 0;

            // Cohesion - move toward centroid
            vx += (centroidX - robot.x) * 0.01;
            vy += (centroidY - robot.y) * 0.01;

            // Separation - avoid others
            for (const other of this.robots) {
                if (other.robot_id === robot.robot_id) continue;
                const dx = robot.x - other.x;
                const dy = robot.y - other.y;
                const dist = Math.sqrt(dx * dx + dy * dy);
                if (dist < 30 && dist > 0) {
                    vx += dx / dist * 2;
                    vy += dy / dist * 2;
                }
            }

            // Mode-specific behavior
            if (this.mode === 4) { // Scatter
                vx += (robot.x - centroidX) * 0.02;
                vy += (robot.y - centroidY) * 0.02;
            } else if (this.mode === 5) { // Gather
                vx += (250 - robot.x) * 0.03;
                vy += (250 - robot.y) * 0.03;
            }

            // Wall avoidance
            if (robot.x < 50) vx += 2;
            if (robot.x > 450) vx -= 2;
            if (robot.y < 50) vy += 2;
            if (robot.y > 450) vy -= 2;

            // Random wandering
            vx += (Math.random() - 0.5) * 2;
            vy += (Math.random() - 0.5) * 2;

            // Limit speed
            const speed = Math.sqrt(vx * vx + vy * vy);
            const maxSpeed = 20;
            if (speed > maxSpeed) {
                vx = vx / speed * maxSpeed;
                vy = vy / speed * maxSpeed;
            }

            // Smooth velocity
            robot.vx = robot.vx * 0.9 + vx * 0.1;
            robot.vy = robot.vy * 0.9 + vy * 0.1;

            // Update position
            robot.x += robot.vx * dt;
            robot.y += robot.vy * dt;

            // Clamp to arena
            robot.x = Math.max(ROBOT_RADIUS, Math.min(ARENA_WIDTH - ROBOT_RADIUS, robot.x));
            robot.y = Math.max(ROBOT_RADIUS, Math.min(ARENA_HEIGHT - ROBOT_RADIUS, robot.y));

            // Update heading
            if (Math.abs(robot.vx) > 0.1 || Math.abs(robot.vy) > 0.1) {
                robot.heading = Math.atan2(robot.vy, robot.vx);
            }

            robot.timestamp_us = Date.now() * 1000;
        }

        // Update robot map
        for (const robot of this.robots) {
            robots.set(robot.robot_id, robot);
        }

        // Update swarm state
        let spread = 0;
        for (const robot of this.robots) {
            const dx = robot.x - centroidX;
            const dy = robot.y - centroidY;
            spread += Math.sqrt(dx * dx + dy * dy);
        }
        spread /= this.robots.length;

        swarmState = {
            robot_count: this.robots.length,
            mode: this.mode,
            formation: this.formation,
            centroid_x: centroidX,
            centroid_y: centroidY,
            spread: spread,
            avg_battery: Math.floor(this.robots.reduce((s, r) => s + r.battery, 0) / this.robots.length),
            timestamp_us: Date.now() * 1000
        };
    }
}

let demoSim = null;

// Initialize
function init() {
    canvas = document.getElementById('arena');
    ctx = canvas.getContext('2d');

    // Check for demo mode
    const params = new URLSearchParams(window.location.search);
    demoMode = params.has('demo');

    if (demoMode) {
        console.log('Running in demo mode (no backend)');
        document.getElementById('connection-status').textContent = 'DEMO MODE';
        document.getElementById('connection-status').className = 'connected';
        demoSim = new DemoSimulator();

        // Add demo obstacles
        obstacles.set(0, { obstacle_id: 0, x: 150, y: 150, radius: 30, obstacle_type: 0 });
        obstacles.set(1, { obstacle_id: 1, x: 350, y: 350, radius: 40, obstacle_type: 0 });
        obstacles.set(2, { obstacle_id: 2, x: 250, y: 400, radius: 25, obstacle_type: 0 });
        obstacles.set(3, { obstacle_id: 3, x: 400, y: 150, radius: 35, obstacle_type: 0 });

        // Add demo waypoints
        waypoints.set(0, { waypoint_id: 0, x: 100, y: 100, radius: 20, sequence: 0 });
        waypoints.set(1, { waypoint_id: 1, x: 400, y: 100, radius: 20, sequence: 1 });
        waypoints.set(2, { waypoint_id: 2, x: 400, y: 400, radius: 20, sequence: 2 });
        waypoints.set(3, { waypoint_id: 3, x: 100, y: 400, radius: 20, sequence: 3 });
        waypoints.set(4, { waypoint_id: 4, x: 250, y: 250, radius: 20, sequence: 4 });
    } else {
        connectWebSocket();
    }

    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);

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
    const container = document.getElementById('arena-container');
    const maxWidth = container.clientWidth - 40;
    const maxHeight = container.clientHeight - 40;

    scale = Math.min(maxWidth / ARENA_WIDTH, maxHeight / ARENA_HEIGHT);
    canvas.width = ARENA_WIDTH * scale;
    canvas.height = ARENA_HEIGHT * scale;
}

function connectWebSocket() {
    const statusEl = document.getElementById('connection-status');
    const HDDS_WS_URL = `ws://${window.location.hostname || 'localhost'}:9090/ws`;
    const TOPICS = ['Robot', 'SwarmState', 'Obstacle', 'Waypoint'];

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
            handleMessage({ topic: msg.topic, data: msg.sample });
            break;
        case 'error':
            console.error(`DDS error [${msg.code}]: ${msg.message}`);
            break;
    }
}

function handleMessage(msg) {
    const { topic, data } = msg;

    switch (topic) {
        case 'Robot':
            robots.set(data.robot_id, data);
            break;
        case 'SwarmState':
            swarmState = data;
            break;
        case 'Obstacle':
            obstacles.set(data.obstacle_id, data);
            break;
        case 'Waypoint':
            waypoints.set(data.waypoint_id, data);
            break;
    }
}

function render() {
    ctx.save();
    ctx.scale(scale, scale);

    // Clear
    ctx.fillStyle = '#080810';
    ctx.fillRect(0, 0, ARENA_WIDTH, ARENA_HEIGHT);

    // Draw grid
    drawGrid();

    // Draw waypoints
    for (const wp of waypoints.values()) {
        drawWaypoint(wp);
    }

    // Draw obstacles
    for (const obs of obstacles.values()) {
        drawObstacle(obs);
    }

    // Draw swarm connections and centroid
    if (swarmState && robots.size > 0) {
        drawSwarmConnections();
        drawCentroid();
    }

    // Draw robots
    for (const robot of robots.values()) {
        drawRobot(robot);
    }

    ctx.restore();
}

function drawGrid() {
    ctx.strokeStyle = '#1a1a25';
    ctx.lineWidth = 1;

    // Grid lines every 50cm
    for (let x = 0; x <= ARENA_WIDTH; x += 50) {
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, ARENA_HEIGHT);
        ctx.stroke();
    }

    for (let y = 0; y <= ARENA_HEIGHT; y += 50) {
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(ARENA_WIDTH, y);
        ctx.stroke();
    }

    // Border
    ctx.strokeStyle = '#2a2a35';
    ctx.lineWidth = 2;
    ctx.strokeRect(0, 0, ARENA_WIDTH, ARENA_HEIGHT);
}

function drawWaypoint(wp) {
    ctx.save();
    ctx.strokeStyle = '#4ecdc4';
    ctx.lineWidth = 2;
    ctx.setLineDash([5, 5]);

    ctx.beginPath();
    ctx.arc(wp.x, wp.y, wp.radius, 0, Math.PI * 2);
    ctx.stroke();

    // Sequence number
    ctx.setLineDash([]);
    ctx.fillStyle = '#4ecdc4';
    ctx.font = '12px sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(wp.sequence.toString(), wp.x, wp.y);

    ctx.restore();
}

function drawObstacle(obs) {
    ctx.save();

    // Shadow
    ctx.fillStyle = 'rgba(255, 107, 107, 0.2)';
    ctx.beginPath();
    ctx.arc(obs.x, obs.y, obs.radius + 5, 0, Math.PI * 2);
    ctx.fill();

    // Main obstacle
    ctx.fillStyle = 'rgba(255, 107, 107, 0.4)';
    ctx.strokeStyle = '#ff6b6b';
    ctx.lineWidth = 2;

    ctx.beginPath();
    ctx.arc(obs.x, obs.y, obs.radius, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();

    ctx.restore();
}

function drawSwarmConnections() {
    ctx.save();
    ctx.strokeStyle = 'rgba(0, 255, 136, 0.1)';
    ctx.lineWidth = 1;

    const robotArray = Array.from(robots.values());
    for (let i = 0; i < robotArray.length; i++) {
        for (let j = i + 1; j < robotArray.length; j++) {
            const r1 = robotArray[i];
            const r2 = robotArray[j];
            const dx = r2.x - r1.x;
            const dy = r2.y - r1.y;
            const dist = Math.sqrt(dx * dx + dy * dy);

            if (dist < 80) {
                ctx.globalAlpha = 1 - dist / 80;
                ctx.beginPath();
                ctx.moveTo(r1.x, r1.y);
                ctx.lineTo(r2.x, r2.y);
                ctx.stroke();
            }
        }
    }

    ctx.restore();
}

function drawCentroid() {
    if (!swarmState) return;

    ctx.save();

    // Spread circle
    ctx.strokeStyle = 'rgba(0, 255, 136, 0.3)';
    ctx.lineWidth = 1;
    ctx.setLineDash([5, 5]);
    ctx.beginPath();
    ctx.arc(swarmState.centroid_x, swarmState.centroid_y, swarmState.spread, 0, Math.PI * 2);
    ctx.stroke();

    // Centroid marker
    ctx.setLineDash([]);
    ctx.strokeStyle = '#00ff88';
    ctx.lineWidth = 2;

    const size = 8;
    ctx.beginPath();
    ctx.moveTo(swarmState.centroid_x - size, swarmState.centroid_y);
    ctx.lineTo(swarmState.centroid_x + size, swarmState.centroid_y);
    ctx.moveTo(swarmState.centroid_x, swarmState.centroid_y - size);
    ctx.lineTo(swarmState.centroid_x, swarmState.centroid_y + size);
    ctx.stroke();

    ctx.restore();
}

function drawRobot(robot) {
    ctx.save();
    ctx.translate(robot.x, robot.y);

    // Color from hex
    const color = `#${robot.color.toString(16).padStart(6, '0')}`;

    // Sensor range (dim)
    if (robot.is_leader) {
        ctx.fillStyle = 'rgba(0, 255, 136, 0.05)';
        ctx.beginPath();
        ctx.arc(0, 0, 50, 0, Math.PI * 2);
        ctx.fill();
    }

    // Velocity trail
    if (Math.abs(robot.vx) > 0.5 || Math.abs(robot.vy) > 0.5) {
        ctx.strokeStyle = `${color}44`;
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.moveTo(0, 0);
        ctx.lineTo(-robot.vx * 0.5, -robot.vy * 0.5);
        ctx.stroke();
    }

    // Robot body
    ctx.rotate(robot.heading);

    // Glow effect
    const gradient = ctx.createRadialGradient(0, 0, 0, 0, 0, ROBOT_RADIUS * 1.5);
    gradient.addColorStop(0, `${color}88`);
    gradient.addColorStop(1, `${color}00`);
    ctx.fillStyle = gradient;
    ctx.beginPath();
    ctx.arc(0, 0, ROBOT_RADIUS * 1.5, 0, Math.PI * 2);
    ctx.fill();

    // Main body
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.arc(0, 0, ROBOT_RADIUS, 0, Math.PI * 2);
    ctx.fill();

    // Heading indicator (arrow)
    ctx.fillStyle = '#ffffff';
    ctx.beginPath();
    ctx.moveTo(ROBOT_RADIUS + 4, 0);
    ctx.lineTo(ROBOT_RADIUS - 2, -3);
    ctx.lineTo(ROBOT_RADIUS - 2, 3);
    ctx.closePath();
    ctx.fill();

    // Leader crown
    if (robot.is_leader) {
        ctx.strokeStyle = '#00ff88';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(0, 0, ROBOT_RADIUS + 3, 0, Math.PI * 2);
        ctx.stroke();
    }

    ctx.restore();

    // Name label
    ctx.fillStyle = '#888899';
    ctx.font = '9px sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'top';
    ctx.fillText(robot.name, robot.x, robot.y + ROBOT_RADIUS + 4);
}

function updateUI() {
    // Swarm info
    if (swarmState) {
        document.getElementById('mode').textContent = MODE_NAMES[swarmState.mode] || '--';
        document.getElementById('formation').textContent = FORMATION_NAMES[swarmState.formation] || '--';
        document.getElementById('robot-count').textContent = swarmState.robot_count;
        document.getElementById('spread').textContent = `${swarmState.spread.toFixed(1)} cm`;
        document.getElementById('avg-battery').textContent = `${swarmState.avg_battery}%`;
    }

    // Robot list
    const container = document.getElementById('robots');
    const sortedRobots = Array.from(robots.values()).sort((a, b) => a.robot_id - b.robot_id);

    let html = '';
    for (const robot of sortedRobots) {
        const color = `#${robot.color.toString(16).padStart(6, '0')}`;
        const batteryClass = robot.battery < 30 ? 'critical' : robot.battery < 50 ? 'low' : '';
        const leaderClass = robot.is_leader ? 'leader' : '';

        html += `
            <div class="robot-item ${leaderClass}">
                <span class="color-dot" style="background: ${color}"></span>
                <span class="name">${robot.name}</span>
                <span class="battery ${batteryClass}">${robot.battery}%</span>
            </div>
        `;
    }
    container.innerHTML = html;
}

// Start
init();
