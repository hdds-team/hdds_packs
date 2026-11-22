/**
 * Metal Slug Arena - Game Client
 *
 * TODO: Add real sprite sheets for better visuals
 * - Download sprites from spriters-resource.com or opengameart.org
 * - Create sprite animation system (frames, timing per state)
 * - States needed: idle, run, jump, fall, shoot, die
 * - Load sprite sheets as Image objects
 * - Use ctx.drawImage() with source rect for frame selection
 */

// Configuration
const HDDS_WS_URL = `ws://${window.location.hostname || 'localhost'}:9090/ws`;
const DEMO_MODE = new URLSearchParams(window.location.search).has('demo');
// Topic names, mirroring the `topics` module of arena-types: the viewer
// subscribes by DDS topic name, not by type name.
const TOPIC = {
    PLAYER: 'arena/Player',
    PROJECTILE: 'arena/Projectile',
    EXPLOSION: 'arena/Explosion',
    POWERUP: 'arena/PowerUp',
    KILL: 'arena/KillEvent',
    GAME_STATE: 'arena/GameState',
};
const TOPICS = Object.values(TOPIC);

// Game constants
const ARENA_WIDTH = 800;
const ARENA_HEIGHT = 600;
const PLAYER_WIDTH = 32;
const PLAYER_HEIGHT = 40;

// Platforms (matching server)
const PLATFORMS = [
    { x: 0, y: 560, w: 800, h: 40 },
    { x: 50, y: 450, w: 150, h: 20 },
    { x: 300, y: 400, w: 200, h: 20 },
    { x: 600, y: 450, w: 150, h: 20 },
    { x: 150, y: 300, w: 120, h: 20 },
    { x: 530, y: 300, w: 120, h: 20 },
    { x: 300, y: 200, w: 200, h: 20 },
];

// Colors
const COLORS = {
    platform: '#4a3728',
    platformTop: '#6b5344',
    redTeam: '#ff4444',
    blueTeam: '#4488ff',
    bullet: '#ffff00',
    rocket: '#ff6600',
    explosion: ['#ffffff', '#ffff00', '#ff8800', '#ff4400'],
    powerup: {
        0: '#44ff44', // Health
        1: '#ff8844', // Heavy MG
        2: '#ff4444', // Rocket
        3: '#8844ff', // Shotgun
        4: '#44ffff', // Grenade
    },
};

// Weapon names
const WEAPON_NAMES = ['PISTOL', 'HEAVY MG', 'ROCKET', 'SHOTGUN', 'LASER', 'FLAME', 'GRENADE'];

// State
let ws = null;
let connected = false;
let localPlayerId = null;
const players = new Map();
const projectiles = new Map();
const powerups = new Map();
const explosions = [];
let gameState = { phase: 0, time_remaining_s: 300, red_score: 0, blue_score: 0 };
let localPlayer = null;

// Input state
const keys = {
    left: false,
    right: false,
    jump: false,
    shoot: false,
};

// DOM elements
const canvas = document.getElementById('game-canvas');
const ctx = canvas.getContext('2d');
const statusDot = document.querySelector('.status-dot');
const statusText = document.querySelector('.status-text');
const playerNameEl = document.getElementById('player-name');
const healthFill = document.getElementById('health-fill');
const weaponInfo = document.getElementById('weapon-info');
const redScoreEl = document.getElementById('red-score');
const blueScoreEl = document.getElementById('blue-score');
const gameTimeEl = document.getElementById('game-time');
const killsEl = document.getElementById('kills');
const deathsEl = document.getElementById('deaths');
const killFeed = document.getElementById('kill-feed');

/**
 * Initialize game
 */
function init() {
    console.log('[ARENA] Metal Slug Arena initializing...');
    console.log('[ARENA] Demo mode:', DEMO_MODE);

    // Setup input handlers
    setupInput();

    if (DEMO_MODE) {
        startDemoMode();
    } else {
        connectWebSocket();
    }

    // Start render loop
    requestAnimationFrame(gameLoop);
}

/**
 * Setup keyboard input
 */
function setupInput() {
    document.addEventListener('keydown', (e) => {
        switch (e.code) {
            case 'ArrowLeft':
            case 'KeyA':
                keys.left = true;
                break;
            case 'ArrowRight':
            case 'KeyD':
                keys.right = true;
                break;
            case 'ArrowUp':
            case 'KeyW':
            case 'Space':
                keys.jump = true;
                e.preventDefault();
                break;
            case 'KeyX':
            case 'KeyZ':
            case 'ControlLeft':
                keys.shoot = true;
                break;
        }
    });

    document.addEventListener('keyup', (e) => {
        switch (e.code) {
            case 'ArrowLeft':
            case 'KeyA':
                keys.left = false;
                break;
            case 'ArrowRight':
            case 'KeyD':
                keys.right = false;
                break;
            case 'ArrowUp':
            case 'KeyW':
            case 'Space':
                keys.jump = false;
                break;
            case 'KeyX':
            case 'KeyZ':
            case 'ControlLeft':
                keys.shoot = false;
                break;
        }
    });
}

/**
 * Connect to WebSocket
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
        case TOPIC.PLAYER:
            players.set(data.player_id, data);
            if (localPlayerId === null || data.player_id === localPlayerId) {
                if (localPlayerId === null) localPlayerId = data.player_id;
                localPlayer = data;
                updateHUD();
            }
            break;

        case TOPIC.PROJECTILE:
            if (data.active === 1) {
                projectiles.set(data.projectile_id, data);
            } else {
                projectiles.delete(data.projectile_id);
            }
            break;

        case TOPIC.EXPLOSION:
            explosions.push({
                ...data,
                frame: 0,
                maxFrames: 10,
            });
            break;

        case TOPIC.POWERUP:
            if (data.active === 1) {
                powerups.set(data.powerup_id, data);
            } else {
                powerups.delete(data.powerup_id);
            }
            break;

        case TOPIC.KILL:
            addKillFeed(data);
            break;

        case TOPIC.GAME_STATE:
            gameState = data;
            updateHUD();
            break;
    }
}

/**
 * Update HUD elements
 */
function updateHUD() {
    if (localPlayer) {
        playerNameEl.textContent = localPlayer.name;

        const healthPct = (localPlayer.health / 100) * 100;
        healthFill.style.width = `${healthPct}%`;
        healthFill.classList.remove('warning', 'critical');
        if (healthPct <= 25) {
            healthFill.classList.add('critical');
        } else if (healthPct <= 50) {
            healthFill.classList.add('warning');
        }

        const weaponName = WEAPON_NAMES[localPlayer.weapon] || 'PISTOL';
        const ammoText = localPlayer.weapon === 0 ? '∞' : localPlayer.ammo;
        weaponInfo.textContent = `${weaponName} [${ammoText}]`;

        killsEl.textContent = localPlayer.kills;
        deathsEl.textContent = localPlayer.deaths;
    }

    redScoreEl.textContent = gameState.red_score;
    blueScoreEl.textContent = gameState.blue_score;

    const mins = Math.floor(gameState.time_remaining_s / 60);
    const secs = gameState.time_remaining_s % 60;
    gameTimeEl.textContent = `${mins}:${secs.toString().padStart(2, '0')}`;
}

/**
 * Add kill to feed
 */
function addKillFeed(kill) {
    const entry = document.createElement('div');
    entry.className = 'kill-entry';
    entry.innerHTML = `<span class="killer">${kill.killer_name}</span> → <span class="victim">${kill.victim_name}</span>`;
    killFeed.insertBefore(entry, killFeed.firstChild);

    // Keep only last 5
    while (killFeed.children.length > 5) {
        killFeed.removeChild(killFeed.lastChild);
    }

    // Auto-remove after 5 seconds
    setTimeout(() => entry.remove(), 5000);
}

/**
 * Update connection UI
 */
function updateConnectionUI(isConnected) {
    statusDot.classList.toggle('connected', isConnected);
    statusText.textContent = isConnected ? 'Connected' : 'Disconnected';
}

/**
 * Main game loop
 */
function gameLoop(timestamp) {
    update();
    render();
    requestAnimationFrame(gameLoop);
}

/**
 * Update game state
 */
function update() {
    // Update explosions
    for (let i = explosions.length - 1; i >= 0; i--) {
        explosions[i].frame++;
        if (explosions[i].frame >= explosions[i].maxFrames) {
            explosions.splice(i, 1);
        }
    }

    // Send input to server (would need DDS writer in real implementation)
    // For now, the server handles bots
}

/**
 * Render game
 */
function render() {
    // Clear
    ctx.fillStyle = '#2a4a6c';
    ctx.fillRect(0, 0, ARENA_WIDTH, ARENA_HEIGHT);

    // Draw background gradient
    const gradient = ctx.createLinearGradient(0, 0, 0, ARENA_HEIGHT);
    gradient.addColorStop(0, '#1a3a5c');
    gradient.addColorStop(0.5, '#2a4a6c');
    gradient.addColorStop(1, '#3a5a7c');
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, ARENA_WIDTH, ARENA_HEIGHT);

    // Draw platforms
    drawPlatforms();

    // Draw power-ups
    drawPowerUps();

    // Draw players
    drawPlayers();

    // Draw projectiles
    drawProjectiles();

    // Draw explosions
    drawExplosions();
}

/**
 * Draw platforms
 */
function drawPlatforms() {
    for (const p of PLATFORMS) {
        // Platform body
        ctx.fillStyle = COLORS.platform;
        ctx.fillRect(p.x, p.y, p.w, p.h);

        // Platform top (grass/metal)
        ctx.fillStyle = COLORS.platformTop;
        ctx.fillRect(p.x, p.y, p.w, 4);

        // Pixel detail
        ctx.fillStyle = '#3a2718';
        for (let i = 0; i < p.w; i += 16) {
            ctx.fillRect(p.x + i + 4, p.y + 8, 8, 4);
        }
    }
}

/**
 * Draw power-ups
 */
function drawPowerUps() {
    for (const [id, powerup] of powerups) {
        const color = COLORS.powerup[powerup.powerup_type] || '#ffffff';

        // Floating animation
        const float = Math.sin(Date.now() / 200 + id) * 3;

        // Glow
        ctx.shadowColor = color;
        ctx.shadowBlur = 10;

        // Box
        ctx.fillStyle = color;
        ctx.fillRect(powerup.x - 10, powerup.y - 10 + float, 20, 20);

        // Letter
        ctx.shadowBlur = 0;
        ctx.fillStyle = '#000';
        ctx.font = 'bold 12px Arial';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        const letters = ['H', 'M', 'R', 'S', 'G', 'SH', 'SP'];
        ctx.fillText(letters[powerup.powerup_type] || '?', powerup.x, powerup.y + float);
    }
    ctx.shadowBlur = 0;
}

/**
 * Draw players
 */
function drawPlayers() {
    for (const [id, player] of players) {
        if (player.state === 6) continue; // Dead

        const isLocal = id === localPlayerId;
        const teamColor = player.team === 1 ? COLORS.redTeam : COLORS.blueTeam;

        // Shadow
        ctx.fillStyle = 'rgba(0,0,0,0.3)';
        ctx.beginPath();
        ctx.ellipse(player.x + PLAYER_WIDTH / 2, player.y + PLAYER_HEIGHT + 2,
                    PLAYER_WIDTH / 2, 5, 0, 0, Math.PI * 2);
        ctx.fill();

        // Body
        ctx.fillStyle = teamColor;
        ctx.fillRect(player.x + 8, player.y + 15, 16, 25);

        // Head
        ctx.fillStyle = '#ffcc99';
        ctx.fillRect(player.x + 10, player.y + 5, 12, 12);

        // Helmet
        ctx.fillStyle = player.team === 1 ? '#aa2222' : '#2222aa';
        ctx.fillRect(player.x + 8, player.y + 2, 16, 8);

        // Eyes
        ctx.fillStyle = '#000';
        const eyeX = player.facing_right ? player.x + 18 : player.x + 12;
        ctx.fillRect(eyeX, player.y + 9, 2, 2);

        // Gun
        ctx.fillStyle = '#444';
        const gunX = player.facing_right ? player.x + PLAYER_WIDTH : player.x - 10;
        const gunW = player.facing_right ? 10 : 10;
        ctx.fillRect(player.facing_right ? player.x + 24 : player.x - 2, player.y + 20, 10, 4);

        // Legs animation
        const legOffset = Math.sin(Date.now() / 100) * 3;
        if (player.state === 1) { // Running
            ctx.fillStyle = teamColor;
            ctx.fillRect(player.x + 10, player.y + 38, 5, 8 + legOffset);
            ctx.fillRect(player.x + 17, player.y + 38, 5, 8 - legOffset);
        } else {
            ctx.fillStyle = teamColor;
            ctx.fillRect(player.x + 10, player.y + 38, 5, 8);
            ctx.fillRect(player.x + 17, player.y + 38, 5, 8);
        }

        // Name tag
        ctx.fillStyle = isLocal ? '#ffff00' : '#ffffff';
        ctx.font = '8px "Press Start 2P", monospace';
        ctx.textAlign = 'center';
        ctx.fillText(player.name, player.x + PLAYER_WIDTH / 2, player.y - 5);

        // Health bar
        if (!isLocal) {
            const barWidth = 30;
            const barHeight = 4;
            const barX = player.x + PLAYER_WIDTH / 2 - barWidth / 2;
            const barY = player.y - 15;

            ctx.fillStyle = '#333';
            ctx.fillRect(barX, barY, barWidth, barHeight);

            const healthPct = player.health / 100;
            ctx.fillStyle = healthPct > 0.5 ? '#44ff44' : healthPct > 0.25 ? '#ffff44' : '#ff4444';
            ctx.fillRect(barX, barY, barWidth * healthPct, barHeight);
        }
    }
}

/**
 * Draw projectiles
 */
function drawProjectiles() {
    for (const [id, proj] of projectiles) {
        if (proj.weapon === 2) {
            // Rocket
            ctx.fillStyle = '#ff6600';
            ctx.save();
            ctx.translate(proj.x, proj.y);
            ctx.rotate(Math.atan2(proj.vy, proj.vx));
            ctx.fillRect(-8, -3, 16, 6);
            ctx.fillStyle = '#ffff00';
            ctx.fillRect(-12, -2, 4, 4);
            ctx.restore();
        } else if (proj.weapon === 3) {
            // Shotgun pellet
            ctx.fillStyle = '#ffcc00';
            ctx.beginPath();
            ctx.arc(proj.x, proj.y, 3, 0, Math.PI * 2);
            ctx.fill();
        } else {
            // Bullet
            ctx.fillStyle = '#ffff00';
            ctx.shadowColor = '#ffff00';
            ctx.shadowBlur = 5;
            ctx.fillRect(proj.x - 4, proj.y - 1, 8, 2);
            ctx.shadowBlur = 0;
        }
    }
}

/**
 * Draw explosions
 */
function drawExplosions() {
    for (const exp of explosions) {
        const progress = exp.frame / exp.maxFrames;
        const radius = exp.size * (1 + progress);
        const alpha = 1 - progress;

        // Multiple circles for effect
        for (let i = 0; i < 4; i++) {
            const r = radius * (0.4 + i * 0.2);
            ctx.beginPath();
            ctx.arc(exp.x, exp.y, r, 0, Math.PI * 2);
            ctx.fillStyle = COLORS.explosion[i] + Math.floor(alpha * 255).toString(16).padStart(2, '0');
            ctx.fill();
        }

        // Particles
        const particleCount = exp.explosion_type === 2 ? 12 : 6;
        for (let i = 0; i < particleCount; i++) {
            const angle = (i / particleCount) * Math.PI * 2 + progress * 2;
            const dist = radius * 1.5 * progress;
            const px = exp.x + Math.cos(angle) * dist;
            const py = exp.y + Math.sin(angle) * dist;

            ctx.fillStyle = `rgba(255, ${Math.floor(200 - progress * 200)}, 0, ${alpha})`;
            ctx.fillRect(px - 2, py - 2, 4, 4);
        }
    }
}

/**
 * Demo mode
 */
function startDemoMode() {
    updateConnectionUI(true);
    statusText.textContent = 'Demo Mode';

    // Create demo players
    const demoPlayers = [
        { player_id: 1, name: 'MARCO', team: 1, x: 100, y: 400, health: 100, weapon: 0, kills: 3, deaths: 1 },
        { player_id: 2, name: 'TARMA', team: 1, x: 200, y: 250, health: 75, weapon: 1, kills: 5, deaths: 2 },
        { player_id: 3, name: 'ERI', team: 2, x: 600, y: 400, health: 50, weapon: 2, kills: 2, deaths: 3 },
        { player_id: 4, name: 'FIO', team: 2, x: 500, y: 150, health: 100, weapon: 3, kills: 4, deaths: 1 },
    ];

    demoPlayers.forEach(p => {
        p.state = 1;
        p.facing_right = p.team === 1 ? 1 : 0;
        p.vx = 0;
        p.vy = 0;
        p.ammo = 50;
        p.score = p.kills * 100;
        players.set(p.player_id, p);
    });

    localPlayerId = 1;
    localPlayer = demoPlayers[0];

    // Demo power-ups
    [
        { powerup_id: 1, powerup_type: 0, x: 100, y: 420, active: 1 },
        { powerup_id: 2, powerup_type: 1, x: 400, y: 370, active: 1 },
        { powerup_id: 3, powerup_type: 2, x: 700, y: 420, active: 1 },
    ].forEach(p => powerups.set(p.powerup_id, p));

    gameState = { phase: 2, time_remaining_s: 245, red_score: 8, blue_score: 6 };
    updateHUD();

    // Animate demo
    setInterval(() => {
        for (const [id, player] of players) {
            // Random movement
            player.x += (Math.random() - 0.5) * 4;
            player.y += Math.sin(Date.now() / 500 + id) * 0.5;
            player.x = Math.max(0, Math.min(ARENA_WIDTH - PLAYER_WIDTH, player.x));

            // Random shooting
            if (Math.random() < 0.02) {
                const dir = player.facing_right ? 1 : -1;
                projectiles.set(Date.now() + id, {
                    projectile_id: Date.now() + id,
                    owner_id: id,
                    weapon: player.weapon,
                    x: player.x + PLAYER_WIDTH / 2 + dir * 20,
                    y: player.y + 20,
                    vx: dir * 600,
                    vy: 0,
                    active: 1,
                });
            }
        }

        // Remove old projectiles
        for (const [id, proj] of projectiles) {
            proj.x += proj.vx * 0.016;
            proj.y += proj.vy * 0.016;
            if (proj.x < 0 || proj.x > ARENA_WIDTH) {
                projectiles.delete(id);
            }
        }

        // Random explosions
        if (Math.random() < 0.01) {
            explosions.push({
                x: Math.random() * ARENA_WIDTH,
                y: Math.random() * ARENA_HEIGHT,
                size: 20 + Math.random() * 30,
                explosion_type: Math.floor(Math.random() * 3),
                frame: 0,
                maxFrames: 10,
            });
        }
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
    state: ['IDLE', 'RUNNING', 'JUMPING', 'FALLING', 'SHOOTING', 'DYING', 'DEAD', 'IN_VEHICLE'],
    weapon: [
        'PISTOL',
        'WEAPON_HEAVY_MG',
        'WEAPON_ROCKET',
        'WEAPON_SHOTGUN',
        'LASER',
        'FLAME',
        'WEAPON_GRENADE',
    ],
    team: ['NONE', 'RED', 'BLUE'],
    powerup_type: [
        'HEALTH',
        'POWERUP_HEAVY_MG',
        'POWERUP_ROCKET',
        'POWERUP_SHOTGUN',
        'POWERUP_GRENADE',
        'SHIELD',
        'SPEED',
    ],
    phase: ['WAITING', 'COUNTDOWN', 'PLAYING', 'ROUND_END', 'GAME_OVER'],
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
