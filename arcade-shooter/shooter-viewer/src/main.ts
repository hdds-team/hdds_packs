// Arcade Shooter DDS Viewer
// Canvas 2D rendering with sprites
// Connects to HDDS via hdds-ws bridge (WebSocket JSON protocol).

const GAME_WIDTH = 800;
const GAME_HEIGHT = 600;

// hdds-ws protocol configuration
const HDDS_WS_URL = `ws://${window.location.hostname || 'localhost'}:9090/ws`;
const SUBSCRIBE_TOPICS = ['shooter/player', 'shooter/alien', 'shooter/bullet', 'shooter/explosion', 'shooter/powerup', 'shooter/game_state', 'shooter/kill_event'];
const INPUT_TOPIC = 'shooter/player_input';
// One player in this demo; the simulator keys its Player sample on this id.
const PLAYER_ID = 1;

// Types matching IDL
interface Player {
    player_id: number;
    x: number;
    y: number;
    lives: number;
    score: number;
    power_up: string;
    power_up_time: number;
    alive: boolean;
}

interface Alien {
    alien_id: number;
    alien_type: string;
    x: number;
    y: number;
    health: number;
    alive: boolean;
    row: number;
    col: number;
}

interface Bullet {
    bullet_id: number;
    x: number;
    y: number;
    velocity_y: number;
    from_player: boolean;
    active: boolean;
}

interface Explosion {
    explosion_id: number;
    x: number;
    y: number;
    frame: number;
    active: boolean;
}

interface PowerUp {
    powerup_id: number;
    power_type: string;
    x: number;
    y: number;
    active: boolean;
}

interface GameState {
    phase: string;
    current_wave: number;
    aliens_remaining: number;
    aliens_total: number;
    alien_speed: number;
    high_score: number;
}

interface KillEvent {
    event_id: number;
    alien_id: number;
    points: number;
    x: number;
    y: number;
}

// Score popup for kill events
interface ScorePopup {
    x: number;
    y: number;
    points: number;
    life: number;
}

// Game state
let player: Player | null = null;
let aliens: Alien[] = [];
let bullets: Bullet[] = [];
let explosions: Explosion[] = [];
let powerups: PowerUp[] = [];
let gameState: GameState | null = null;
let scorePopups: ScorePopup[] = [];

// Input state
const keys: Record<string, boolean> = {};

// Stars background
interface Star {
    x: number;
    y: number;
    size: number;
    speed: number;
    brightness: number;
}
const stars: Star[] = [];

// Sprites
const sprites: Record<string, HTMLImageElement> = {};
const spriteNames = [
    'player', 'alien1', 'alien2', 'alien3', 'boss',
    'bullet_player', 'bullet_alien',
    'explosion1', 'explosion2', 'explosion3', 'explosion4',
    'powerup'
];

// DOM elements
const canvas = document.getElementById('game-canvas') as HTMLCanvasElement;
const ctx = canvas.getContext('2d')!;
const connectionStatus = document.getElementById('connection-status')!;
const gameOverDiv = document.getElementById('game-over')!;
const waveClearDiv = document.getElementById('wave-clear')!;

// WebSocket
let ws: WebSocket | null = null;
let demoMode = new URLSearchParams(window.location.search).has('demo');

// Load sprites
async function loadSprites(): Promise<void> {
    const promises = spriteNames.map(name => {
        return new Promise<void>((resolve) => {
            const img = new Image();
            img.onload = () => {
                sprites[name] = img;
                resolve();
            };
            img.onerror = () => {
                console.warn(`Failed to load sprite: ${name}`);
                resolve();
            };
            img.src = `/sprites/${name}.png`;
        });
    });
    await Promise.all(promises);
    console.log('Sprites loaded:', Object.keys(sprites));
}

// Initialize stars
function initStars() {
    for (let i = 0; i < 100; i++) {
        stars.push({
            x: Math.random() * GAME_WIDTH,
            y: Math.random() * GAME_HEIGHT,
            size: Math.random() * 2 + 0.5,
            speed: Math.random() * 0.5 + 0.1,
            brightness: Math.random(),
        });
    }
}

// Connect to WebSocket
function connect() {
    if (demoMode) {
        connectionStatus.textContent = 'DEMO MODE';
        connectionStatus.className = 'demo';
        initDemoMode();
        return;
    }

    console.log(`[WS] Connecting to ${HDDS_WS_URL}...`);

    ws = new WebSocket(HDDS_WS_URL);

    ws.onopen = () => {
        console.log('[WS] Connected to hdds-ws bridge');
        connectionStatus.textContent = 'CONNECTED';
        connectionStatus.className = 'connected';

        // Subscribe to all DDS topics via hdds-ws protocol
        for (const topic of SUBSCRIBE_TOPICS) {
            ws!.send(JSON.stringify({ type: 'subscribe', topic }));
        }
    };

    ws.onclose = () => {
        console.log('[WS] Disconnected');
        connectionStatus.textContent = 'DISCONNECTED';
        connectionStatus.className = '';
        setTimeout(connect, 2000);
    };

    ws.onerror = () => {
        console.error('[WS] Connection error');
        connectionStatus.textContent = 'CONNECTION ERROR';
        connectionStatus.className = '';
    };

    ws.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            handleWsMessage(msg);
        } catch (e) {
            console.error('[WS] Parse error:', e);
        }
    };
}

/**
 * Handle hdds-ws protocol messages
 */
function handleWsMessage(msg: any) {
    switch (msg.type) {
        case 'welcome':
            console.log(`[WS] Connected to DDS domain ${msg.domain} (hdds-ws v${msg.version})`);
            break;
        case 'subscribed':
            console.log(`[WS] Subscribed to ${msg.topic}`);
            break;
        case 'data':
            handleMessage(msg.topic, msg.sample);
            break;
        case 'published':
            // Input acknowledged
            break;
        case 'error':
            console.error(`[WS] DDS error [${msg.code}]: ${msg.message}`);
            break;
    }
}

function handleMessage(topic: string, data: unknown) {
    switch (topic) {
        case 'shooter/player':
            player = data as Player;
            updateHUD();
            break;
        case 'shooter/alien':
            aliens = data as Alien[];
            break;
        case 'shooter/bullet':
            bullets = data as Bullet[];
            break;
        case 'shooter/explosion':
            explosions = data as Explosion[];
            break;
        case 'shooter/powerup':
            powerups = data as PowerUp[];
            break;
        case 'shooter/game_state':
            gameState = data as GameState;
            updateHUD();
            updateOverlays();
            break;
        case 'shooter/kill_event':
            const event = data as KillEvent;
            scorePopups.push({
                x: event.x,
                y: event.y,
                points: event.points,
                life: 1.0
            });
            break;
    }
}

function updateHUD() {
    document.getElementById('score-value')!.textContent = player?.score?.toString() ?? '0';
    document.getElementById('wave-value')!.textContent = gameState?.current_wave?.toString() ?? '1';
    document.getElementById('lives-value')!.textContent = player?.lives?.toString() ?? '3';
    document.getElementById('high-score-value')!.textContent = gameState?.high_score?.toString() ?? '0';
}

function updateOverlays() {
    if (!gameState) return;

    if (gameState.phase === 'PHASE_GAME_OVER') {
        gameOverDiv.classList.remove('hidden');
        waveClearDiv.classList.add('hidden');
        document.getElementById('final-score')!.textContent = player?.score?.toString() ?? '0';
    } else if (gameState.phase === 'PHASE_WAVE_CLEAR') {
        waveClearDiv.classList.remove('hidden');
        gameOverDiv.classList.add('hidden');
        document.getElementById('next-wave')!.textContent = (gameState.current_wave + 1).toString();
    } else {
        gameOverDiv.classList.add('hidden');
        waveClearDiv.classList.add('hidden');
    }
}

// Send input to server via hdds-ws publish
// PlayerInput on the wire: XCDR1 little-endian, members in IDL order with
// natural alignment, no header.
//
//   uint32  player_id     offset 0
//   int8    move_x        offset 4
//   boolean fire          offset 5
//   (two padding bytes, so the next member starts on 8)
//   uint64  timestamp_us  offset 8
//
// The bridge's JSON publish path encodes a single string member, which a typed
// reader cannot decode, so the sample is encoded here and handed over as the
// base64 bytes of a `_raw` field, which the bridge passes through unchanged.
function encodePlayerInput(playerId: number, moveX: number, fire: boolean): string {
    const buffer = new ArrayBuffer(16);
    const view = new DataView(buffer);
    view.setUint32(0, playerId, true);
    view.setInt8(4, moveX);
    view.setUint8(5, fire ? 1 : 0);
    view.setBigUint64(8, BigInt(Date.now()) * 1000n, true);
    let binary = '';
    for (const byte of new Uint8Array(buffer)) binary += String.fromCharCode(byte);
    return btoa(binary);
}

function sendInput() {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;

    let moveX = 0;
    if (keys['ArrowLeft'] || keys['KeyA']) moveX = -1;
    if (keys['ArrowRight'] || keys['KeyD']) moveX = 1;

    const fire = keys['Space'] || keys['KeyW'] || keys['ArrowUp'];

    ws.send(JSON.stringify({
        type: 'publish',
        topic: INPUT_TOPIC,
        data: { _raw: encodePlayerInput(PLAYER_ID, moveX, fire) }
    }));
}

// Restarting the live game is not wired: PlayerInput carries a direction and a
// fire flag, nothing else, and the simulator exposes no restart. The key still
// restarts the offline demo.
function sendRestart() {
    if (demoMode) {
        initDemoState();
    }
}

// Drawing functions
function drawStars() {
    for (const star of stars) {
        // Update position (parallax scroll)
        star.y += star.speed;
        if (star.y > GAME_HEIGHT) {
            star.y = 0;
            star.x = Math.random() * GAME_WIDTH;
        }

        // Twinkle
        star.brightness = 0.3 + Math.sin(Date.now() * 0.005 + star.x) * 0.35 + 0.35;

        ctx.fillStyle = `rgba(255, 255, 255, ${star.brightness})`;
        ctx.beginPath();
        ctx.arc(star.x, star.y, star.size, 0, Math.PI * 2);
        ctx.fill();
    }
}

function drawPlayer() {
    if (!player || !player.alive) return;

    const sprite = sprites['player'];
    if (sprite) {
        const w = 48;
        const h = 48;
        ctx.drawImage(sprite, player.x - w / 2, player.y - h / 2, w, h);

        // Shield effect
        if (player.power_up === 'POWER_SHIELD') {
            ctx.strokeStyle = 'rgba(0, 255, 255, 0.5)';
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.arc(player.x, player.y, 30, 0, Math.PI * 2);
            ctx.stroke();
        }
    } else {
        // Fallback
        ctx.fillStyle = '#00ffff';
        ctx.fillRect(player.x - 20, player.y - 15, 40, 30);
    }
}

function drawAliens() {
    for (const alien of aliens) {
        if (!alien.alive) continue;

        let spriteName: string;
        let size = 40;

        switch (alien.alien_type) {
            case 'ALIEN_BASIC':
                spriteName = 'alien1';
                break;
            case 'ALIEN_FAST':
                spriteName = 'alien2';
                break;
            case 'ALIEN_HEAVY':
                spriteName = 'alien3';
                size = 44;
                break;
            case 'ALIEN_BOSS':
                spriteName = 'boss';
                size = 64;
                break;
            default:
                spriteName = 'alien1';
        }

        const sprite = sprites[spriteName];
        if (sprite) {
            ctx.drawImage(sprite, alien.x - size / 2, alien.y - size / 2, size, size);

            // Health indicator for heavy aliens
            if (alien.health > 1) {
                ctx.fillStyle = '#00ff00';
                ctx.fillRect(alien.x - 10, alien.y + size / 2 + 2, 20 * (alien.health / 2), 3);
            }
        } else {
            // Fallback
            ctx.fillStyle = alien.alien_type === 'ALIEN_HEAVY' ? '#ff00ff' : '#00ff00';
            ctx.fillRect(alien.x - 15, alien.y - 15, 30, 30);
        }
    }
}

function drawBullets() {
    for (const bullet of bullets) {
        if (!bullet.active) continue;

        const spriteName = bullet.from_player ? 'bullet_player' : 'bullet_alien';
        const sprite = sprites[spriteName];

        if (sprite) {
            const w = bullet.from_player ? 8 : 12;
            const h = bullet.from_player ? 20 : 16;
            ctx.drawImage(sprite, bullet.x - w / 2, bullet.y - h / 2, w, h);
        } else {
            ctx.fillStyle = bullet.from_player ? '#ffff00' : '#ff3333';
            ctx.fillRect(bullet.x - 2, bullet.y - 8, 4, 16);
        }
    }
}

function drawExplosions() {
    for (const exp of explosions) {
        if (!exp.active) continue;

        const frame = Math.min(exp.frame, 3);
        const sprite = sprites[`explosion${frame + 1}`];

        if (sprite) {
            const size = 48 + frame * 8;
            ctx.globalAlpha = 1 - frame * 0.2;
            ctx.drawImage(sprite, exp.x - size / 2, exp.y - size / 2, size, size);
            ctx.globalAlpha = 1;
        } else {
            // Fallback
            ctx.fillStyle = `rgba(255, ${150 - frame * 50}, 0, ${1 - frame * 0.25})`;
            const size = 20 + frame * 10;
            ctx.beginPath();
            ctx.arc(exp.x, exp.y, size, 0, Math.PI * 2);
            ctx.fill();
        }
    }
}

function drawPowerups() {
    for (const pu of powerups) {
        if (!pu.active) continue;

        const sprite = sprites['powerup'];
        if (sprite) {
            const size = 24 + Math.sin(Date.now() * 0.01) * 4;
            ctx.drawImage(sprite, pu.x - size / 2, pu.y - size / 2, size, size);
        } else {
            ctx.fillStyle = '#ffd700';
            ctx.beginPath();
            ctx.arc(pu.x, pu.y, 12, 0, Math.PI * 2);
            ctx.fill();
        }
    }
}

function drawScorePopups() {
    for (let i = scorePopups.length - 1; i >= 0; i--) {
        const popup = scorePopups[i];
        popup.life -= 0.02;
        popup.y -= 1;

        if (popup.life <= 0) {
            scorePopups.splice(i, 1);
            continue;
        }

        ctx.globalAlpha = popup.life;
        ctx.fillStyle = '#ffff00';
        ctx.font = '12px "Press Start 2P", monospace';
        ctx.textAlign = 'center';
        ctx.fillText(`+${popup.points}`, popup.x, popup.y);
        ctx.globalAlpha = 1;
    }
}

function drawPowerUpIndicator() {
    if (!player || player.power_up === 'POWER_NONE') return;

    let text = '';
    let color = '#ffffff';

    switch (player.power_up) {
        case 'POWER_RAPID_FIRE':
            text = 'RAPID FIRE';
            color = '#ff6600';
            break;
        case 'POWER_SHIELD':
            text = 'SHIELD';
            color = '#00ffff';
            break;
        case 'POWER_TRIPLE_SHOT':
            text = 'TRIPLE SHOT';
            color = '#ff00ff';
            break;
    }

    ctx.fillStyle = color;
    ctx.font = '10px "Press Start 2P", monospace';
    ctx.textAlign = 'center';
    ctx.fillText(text, GAME_WIDTH / 2, GAME_HEIGHT - 20);

    // Timer bar
    const barWidth = 100;
    const barX = GAME_WIDTH / 2 - barWidth / 2;
    ctx.fillStyle = 'rgba(255, 255, 255, 0.3)';
    ctx.fillRect(barX, GAME_HEIGHT - 15, barWidth, 5);
    ctx.fillStyle = color;
    ctx.fillRect(barX, GAME_HEIGHT - 15, barWidth * (player.power_up_time / 10), 5);
}

// Main render loop
function render() {
    // Clear
    ctx.fillStyle = '#0a0a1a';
    ctx.fillRect(0, 0, GAME_WIDTH, GAME_HEIGHT);

    // Draw layers
    drawStars();
    drawPowerups();
    drawAliens();
    drawBullets();
    drawPlayer();
    drawExplosions();
    drawScorePopups();
    drawPowerUpIndicator();

    requestAnimationFrame(render);
}

// Input handling
document.addEventListener('keydown', (e) => {
    keys[e.code] = true;

    if (e.code === 'Space') {
        e.preventDefault();
        if (gameState?.phase === 'PHASE_GAME_OVER') {
            sendRestart();
        }
    }

    if (e.code === 'KeyR') {
        sendRestart();
    }
});

document.addEventListener('keyup', (e) => {
    keys[e.code] = false;
});

// Send input at regular intervals
setInterval(sendInput, 1000 / 60);

// Demo mode
let demoTime = 0;

function initDemoMode() {
    initDemoState();
    setInterval(updateDemo, 1000 / 60);
}

function initDemoState() {
    player = {
        player_id: 1,
        x: GAME_WIDTH / 2,
        y: 550,
        lives: 3,
        score: 0,
        power_up: 'POWER_NONE',
        power_up_time: 0,
        alive: true,
    };

    gameState = {
        phase: 'PHASE_PLAYING',
        current_wave: 1,
        aliens_remaining: 55,
        aliens_total: 55,
        alien_speed: 1.0,
        high_score: 0,
    };

    aliens = [];
    let id = 1;
    for (let row = 0; row < 5; row++) {
        for (let col = 0; col < 11; col++) {
            const alienType = row === 0 ? 'ALIEN_HEAVY' : row < 3 ? 'ALIEN_FAST' : 'ALIEN_BASIC';
            aliens.push({
                alien_id: id++,
                alien_type: alienType,
                x: 100 + col * 50,
                y: 80 + row * 40,
                health: alienType === 'ALIEN_HEAVY' ? 2 : 1,
                alive: true,
                row,
                col,
            });
        }
    }

    bullets = [];
    explosions = [];
    powerups = [];
    demoTime = 0;
}

function updateDemo() {
    if (!player || !gameState || gameState.phase !== 'PHASE_PLAYING') return;

    demoTime++;
    const dt = 1 / 60;

    // Move player with keys (demo mode is playable!)
    let moveX = 0;
    if (keys['ArrowLeft'] || keys['KeyA']) moveX = -1;
    if (keys['ArrowRight'] || keys['KeyD']) moveX = 1;

    player.x += moveX * 300 * dt;
    player.x = Math.max(30, Math.min(GAME_WIDTH - 30, player.x));

    // Fire
    if ((keys['Space'] || keys['ArrowUp']) && demoTime % 15 === 0) {
        bullets.push({
            bullet_id: demoTime,
            x: player.x,
            y: player.y - 20,
            velocity_y: -500,
            from_player: true,
            active: true,
        });
    }

    // Update bullets
    for (const bullet of bullets) {
        bullet.y += bullet.velocity_y * dt;
        if (bullet.y < -10 || bullet.y > GAME_HEIGHT + 10) {
            bullet.active = false;
        }
    }
    bullets = bullets.filter(b => b.active);

    // Move aliens
    const aliveAliens = aliens.filter(a => a.alive);
    if (demoTime % 30 === 0 && aliveAliens.length > 0) {
        const dir = Math.sin(demoTime * 0.01) > 0 ? 1 : -1;
        for (const alien of aliveAliens) {
            alien.x += dir * 10;
        }
    }

    // Alien shooting
    if (demoTime % 60 === 0 && aliveAliens.length > 0) {
        const shooter = aliveAliens[Math.floor(Math.random() * aliveAliens.length)];
        bullets.push({
            bullet_id: demoTime + 10000,
            x: shooter.x,
            y: shooter.y + 20,
            velocity_y: 200,
            from_player: false,
            active: true,
        });
    }

    // Collisions: player bullets vs aliens
    for (const bullet of bullets) {
        if (!bullet.from_player || !bullet.active) continue;

        for (const alien of aliens) {
            if (!alien.alive) continue;

            const dx = Math.abs(bullet.x - alien.x);
            const dy = Math.abs(bullet.y - alien.y);

            if (dx < 24 && dy < 24) {
                bullet.active = false;
                alien.health--;

                if (alien.health <= 0) {
                    alien.alive = false;
                    gameState!.aliens_remaining--;

                    const points = alien.alien_type === 'ALIEN_BASIC' ? 10 :
                        alien.alien_type === 'ALIEN_FAST' ? 20 : 30;
                    player!.score += points;

                    scorePopups.push({ x: alien.x, y: alien.y, points, life: 1 });

                    explosions.push({
                        explosion_id: demoTime,
                        x: alien.x,
                        y: alien.y,
                        frame: 0,
                        active: true,
                    });
                }
                break;
            }
        }
    }

    // Collisions: alien bullets vs player
    for (const bullet of bullets) {
        if (bullet.from_player || !bullet.active) continue;

        const dx = Math.abs(bullet.x - player.x);
        const dy = Math.abs(bullet.y - player.y);

        if (dx < 20 && dy < 20) {
            bullet.active = false;
            player.lives--;

            explosions.push({
                explosion_id: demoTime + 5000,
                x: player.x,
                y: player.y,
                frame: 0,
                active: true,
            });

            if (player.lives <= 0) {
                player.alive = false;
                gameState!.phase = 'PHASE_GAME_OVER';
            }
        }
    }

    // Update explosions
    for (const exp of explosions) {
        exp.frame++;
        if (exp.frame >= 4) {
            exp.active = false;
        }
    }
    explosions = explosions.filter(e => e.active);

    // Check wave clear
    if (gameState.aliens_remaining === 0) {
        gameState.phase = 'PHASE_WAVE_CLEAR';
        setTimeout(() => {
            if (gameState) {
                gameState.current_wave++;
                initDemoState();
                gameState.current_wave = Math.min(gameState.current_wave, 5);
            }
        }, 2000);
    }

    // Update HUD
    gameState.high_score = Math.max(gameState.high_score, player.score);
    updateHUD();
    updateOverlays();
}

// Start
async function init() {
    await loadSprites();
    initStars();
    connect();
    render();
}

init();
