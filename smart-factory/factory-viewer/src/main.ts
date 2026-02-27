// Smart Factory Viewer - Isometric 2.5D
// Connects to HDDS via hdds-ws bridge (WebSocket JSON protocol).

// Types matching the Rust simulator's JSON output
interface MachineJson {
    machine_id: number;
    name: string;
    machine_type: number;
    state: number;
    grid_x: number;
    grid_y: number;
    temperature_c: number;
    vibration_mm_s: number;
    power_kw: number;
    efficiency_pct: number;
    items_processed: number;
    items_rejected: number;
    cycle_time_s: number;
    timestamp_us: number;
}

interface ConveyorJson {
    conveyor_id: number;
    state: number;
    start_x: number;
    start_y: number;
    end_x: number;
    end_y: number;
    speed_m_s: number;
    capacity: number;
    current_items: number;
    timestamp_us: number;
}

interface ProductJson {
    product_id: number;
    product_type: number;
    quality_pct: number;
    on_conveyor_id: number;
    in_machine_id: number;
    conveyor_position: number;
    batch_id: number;
    timestamp_us: number;
}

interface AlertJson {
    alert_id: number;
    machine_id: number;
    severity: number;
    message: string;
    acknowledged: boolean;
    timestamp_us: number;
}

interface FactoryStateJson {
    machines_total: number;
    machines_working: number;
    machines_idle: number;
    machines_fault: number;
    products_in_progress: number;
    products_completed: number;
    products_rejected: number;
    throughput_per_hour: number;
    overall_efficiency_pct: number;
    total_power_kw: number;
    active_alerts: number;
    timestamp_us: number;
}

// hdds-ws protocol configuration
const HDDS_WS_URL = `ws://${window.location.hostname || 'localhost'}:9090/ws`;
const TOPICS = ['factory/machine', 'factory/conveyor', 'factory/product', 'factory/alert', 'factory/factory_state'];

// State
const machines = new Map<number, MachineJson>();
const conveyors = new Map<number, ConveyorJson>();
const products = new Map<number, ProductJson>();
const alerts = new Map<number, AlertJson>();
let factoryState: FactoryStateJson | null = null;

// View settings
const view = {
    showMachines: true,
    showConveyors: true,
    showProducts: true,
};

// Canvas
let canvas: HTMLCanvasElement;
let ctx: CanvasRenderingContext2D;

// Isometric settings
const ISO = {
    tileWidth: 100,
    tileHeight: 50,
    originX: 0,
    originY: 0,
};

// Machine types and states
const MACHINE_TYPES = ['CNC', 'WELDER', 'ASSEMBLER', 'PAINTER', 'INSPECTOR', 'PACKAGER'];
const MACHINE_STATES = ['IDLE', 'WORKING', 'BLOCKED', 'STARVED', 'MAINTENANCE', 'FAULT'];
const STATE_COLORS: Record<number, string> = {
    0: '#4a5568', // IDLE
    1: '#00d4ff', // WORKING
    2: '#ffaa00', // BLOCKED
    3: '#ffaa00', // STARVED
    4: '#9333ea', // MAINTENANCE
    5: '#ff4466', // FAULT
};

// WebSocket
let ws: WebSocket | null = null;
let reconnectTimer: number | null = null;

function init() {
    canvas = document.getElementById('factory-canvas') as HTMLCanvasElement;
    ctx = canvas.getContext('2d')!;

    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);

    setupControls();
    connectWebSocket();
    requestAnimationFrame(render);
}

function resizeCanvas() {
    const container = canvas.parentElement!;
    canvas.width = container.clientWidth;
    canvas.height = container.clientHeight;

    // Center the isometric grid
    ISO.originX = canvas.width / 2;
    ISO.originY = 150;
}

function setupControls() {
    document.getElementById('btn-machines')?.addEventListener('click', (e) => {
        view.showMachines = !view.showMachines;
        (e.target as HTMLElement).classList.toggle('active', view.showMachines);
    });
    document.getElementById('btn-conveyors')?.addEventListener('click', (e) => {
        view.showConveyors = !view.showConveyors;
        (e.target as HTMLElement).classList.toggle('active', view.showConveyors);
    });
    document.getElementById('btn-products')?.addEventListener('click', (e) => {
        view.showProducts = !view.showProducts;
        (e.target as HTMLElement).classList.toggle('active', view.showProducts);
    });
}

function connectWebSocket() {
    const params = new URLSearchParams(window.location.search);
    if (params.get('demo') === '1') {
        console.log('Demo mode - no WebSocket');
        setStatus(true);
        startDemoMode();
        return;
    }

    console.log(`[WS] Connecting to ${HDDS_WS_URL}...`);

    ws = new WebSocket(HDDS_WS_URL);

    ws.onopen = () => {
        console.log('[WS] Connected to hdds-ws bridge');
        setStatus(true);

        // Subscribe to all DDS topics via hdds-ws protocol
        for (const topic of TOPICS) {
            ws!.send(JSON.stringify({ type: 'subscribe', topic }));
        }

        if (reconnectTimer) {
            clearTimeout(reconnectTimer);
            reconnectTimer = null;
        }
    };

    ws.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            handleWsMessage(msg);
        } catch (e) {
            console.error('[WS] Parse error:', e);
        }
    };

    ws.onclose = () => {
        console.log('[WS] Disconnected');
        setStatus(false);
        ws = null;
        reconnectTimer = window.setTimeout(connectWebSocket, 2000);
    };

    ws.onerror = (e) => {
        console.error('[WS] Error:', e);
    };
}

function setStatus(connected: boolean) {
    const el = document.getElementById('connection-status')!;
    el.textContent = connected ? 'ONLINE' : 'OFFLINE';
    el.className = connected ? 'connected' : 'disconnected';
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
            handleDdsData(msg.topic, msg.sample);
            break;
        case 'error':
            console.error(`[WS] DDS error [${msg.code}]: ${msg.message}`);
            break;
    }
}

/**
 * Handle incoming DDS data by topic
 */
function handleDdsData(topic: string, data: unknown) {
    switch (topic) {
        case 'factory/machine': {
            const m = data as MachineJson;
            machines.set(m.machine_id, m);
            updateMachinesList();
            break;
        }
        case 'factory/conveyor': {
            const c = data as ConveyorJson;
            conveyors.set(c.conveyor_id, c);
            break;
        }
        case 'factory/product': {
            const p = data as ProductJson;
            products.set(p.product_id, p);
            break;
        }
        case 'factory/alert': {
            const a = data as AlertJson;
            if (!a.acknowledged) {
                alerts.set(a.alert_id, a);
            } else {
                alerts.delete(a.alert_id);
            }
            updateAlertsList();
            break;
        }
        case 'factory/factory_state': {
            factoryState = data as FactoryStateJson;
            updateFactoryStats();
            break;
        }
    }
}

function updateFactoryStats() {
    if (!factoryState) return;
    document.getElementById('machines-total')!.textContent = String(factoryState.machines_total);
    document.getElementById('machines-working')!.textContent = String(factoryState.machines_working);
    document.getElementById('machines-idle')!.textContent = String(factoryState.machines_idle);
    document.getElementById('machines-fault')!.textContent = String(factoryState.machines_fault);
    document.getElementById('products-in-progress')!.textContent = String(factoryState.products_in_progress);
    document.getElementById('products-completed')!.textContent = String(factoryState.products_completed);
    document.getElementById('products-rejected')!.textContent = String(factoryState.products_rejected);
    document.getElementById('throughput')!.textContent = `${factoryState.throughput_per_hour.toFixed(1)} /hr`;
    document.getElementById('efficiency')!.textContent = `${factoryState.overall_efficiency_pct.toFixed(0)}%`;
    document.getElementById('power')!.textContent = `${factoryState.total_power_kw.toFixed(1)} kW`;
}

function updateMachinesList() {
    const container = document.getElementById('machine-items')!;
    container.innerHTML = '';
    machines.forEach((m) => {
        const div = document.createElement('div');
        const stateClass = m.state === 1 ? 'working' : m.state === 5 ? 'fault' : '';
        div.className = `machine-item ${stateClass}`;
        div.innerHTML = `
            <div class="name">${m.name}</div>
            <div class="status">${MACHINE_STATES[m.state] || 'UNKNOWN'} - ${m.items_processed} items</div>
            <div class="telemetry">${m.temperature_c.toFixed(1)}C | ${m.vibration_mm_s.toFixed(2)} mm/s | ${m.power_kw.toFixed(1)} kW</div>
        `;
        container.appendChild(div);
    });
}

function updateAlertsList() {
    const container = document.getElementById('alert-items')!;
    container.innerHTML = '';
    if (alerts.size === 0) {
        container.innerHTML = '<div style="color: var(--text-secondary); font-size: 0.85rem;">No active alerts</div>';
        return;
    }
    alerts.forEach((a) => {
        const div = document.createElement('div');
        div.className = `alert-item ${a.severity === 1 ? 'warning' : ''}`;
        div.textContent = a.message;
        container.appendChild(div);
    });
}

// Isometric coordinate conversion
function gridToIso(gx: number, gy: number): { x: number; y: number } {
    const x = ISO.originX + (gx - gy) * (ISO.tileWidth / 2);
    const y = ISO.originY + (gx + gy) * (ISO.tileHeight / 2);
    return { x, y };
}

// Rendering
function render() {
    // Clear
    ctx.fillStyle = '#1a1a2e';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // Draw floor grid
    drawFloorGrid();

    // Draw conveyors (below machines)
    if (view.showConveyors) drawConveyors();

    // Draw machines
    if (view.showMachines) drawMachines();

    // Draw products
    if (view.showProducts) drawProducts();

    requestAnimationFrame(render);
}

function drawFloorGrid() {
    ctx.strokeStyle = 'rgba(0, 212, 255, 0.1)';
    ctx.lineWidth = 1;

    for (let gx = -1; gx <= 6; gx++) {
        for (let gy = -1; gy <= 4; gy++) {
            const { x, y } = gridToIso(gx, gy);
            drawIsoDiamond(x, y, 'rgba(0, 212, 255, 0.03)', 'rgba(0, 212, 255, 0.08)');
        }
    }
}

function drawIsoDiamond(x: number, y: number, fill: string, stroke: string) {
    ctx.beginPath();
    ctx.moveTo(x, y - ISO.tileHeight / 2);
    ctx.lineTo(x + ISO.tileWidth / 2, y);
    ctx.lineTo(x, y + ISO.tileHeight / 2);
    ctx.lineTo(x - ISO.tileWidth / 2, y);
    ctx.closePath();
    ctx.fillStyle = fill;
    ctx.fill();
    ctx.strokeStyle = stroke;
    ctx.stroke();
}

function drawConveyors() {
    ctx.lineWidth = 8;
    ctx.lineCap = 'round';

    conveyors.forEach((c) => {
        const start = gridToIso(c.start_x, c.start_y);
        const end = gridToIso(c.end_x, c.end_y);

        // Conveyor belt
        ctx.strokeStyle = '#4a5568';
        ctx.beginPath();
        ctx.moveTo(start.x, start.y);
        ctx.lineTo(end.x, end.y);
        ctx.stroke();

        // Belt stripes animation
        const time = Date.now() / 1000;
        const stripeOffset = (time * 50) % 20;
        ctx.setLineDash([10, 10]);
        ctx.lineDashOffset = -stripeOffset;
        ctx.strokeStyle = '#6b7280';
        ctx.lineWidth = 4;
        ctx.beginPath();
        ctx.moveTo(start.x, start.y);
        ctx.lineTo(end.x, end.y);
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.lineWidth = 8;
    });
}

function drawMachines() {
    // Sort by grid position for proper depth ordering
    const sortedMachines = Array.from(machines.values()).sort((a, b) => {
        return (a.grid_x + a.grid_y) - (b.grid_x + b.grid_y);
    });

    sortedMachines.forEach((m) => {
        const { x, y } = gridToIso(m.grid_x, m.grid_y);
        drawIsometricBox(x, y, m);
    });
}

function drawIsometricBox(x: number, y: number, m: MachineJson) {
    const w = ISO.tileWidth * 0.7;
    const h = ISO.tileHeight * 0.7;
    const boxHeight = 40;

    const stateColor = STATE_COLORS[m.state] || '#4a5568';
    const isWorking = m.state === 1;
    const isFault = m.state === 5;

    // Top face
    ctx.beginPath();
    ctx.moveTo(x, y - boxHeight - h / 2);
    ctx.lineTo(x + w / 2, y - boxHeight);
    ctx.lineTo(x, y - boxHeight + h / 2);
    ctx.lineTo(x - w / 2, y - boxHeight);
    ctx.closePath();
    ctx.fillStyle = isWorking ? '#00a8cc' : isFault ? '#cc3344' : '#3a4558';
    ctx.fill();
    ctx.strokeStyle = stateColor;
    ctx.lineWidth = 2;
    ctx.stroke();

    // Right face
    ctx.beginPath();
    ctx.moveTo(x + w / 2, y - boxHeight);
    ctx.lineTo(x + w / 2, y);
    ctx.lineTo(x, y + h / 2);
    ctx.lineTo(x, y - boxHeight + h / 2);
    ctx.closePath();
    ctx.fillStyle = isWorking ? '#007799' : isFault ? '#992233' : '#2a3548';
    ctx.fill();
    ctx.stroke();

    // Left face
    ctx.beginPath();
    ctx.moveTo(x - w / 2, y - boxHeight);
    ctx.lineTo(x - w / 2, y);
    ctx.lineTo(x, y + h / 2);
    ctx.lineTo(x, y - boxHeight + h / 2);
    ctx.closePath();
    ctx.fillStyle = isWorking ? '#005566' : isFault ? '#771122' : '#1a2538';
    ctx.fill();
    ctx.stroke();

    // Glow effect for working machines
    if (isWorking) {
        ctx.shadowColor = '#00d4ff';
        ctx.shadowBlur = 15;
        ctx.strokeStyle = '#00d4ff';
        ctx.stroke();
        ctx.shadowBlur = 0;
    }

    // Fault indicator
    if (isFault && Math.floor(Date.now() / 500) % 2 === 0) {
        ctx.fillStyle = '#ff4466';
        ctx.beginPath();
        ctx.arc(x, y - boxHeight - h / 2 - 15, 8, 0, Math.PI * 2);
        ctx.fill();
    }

    // Machine label
    ctx.fillStyle = '#ffffff';
    ctx.font = 'bold 11px sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText(m.name, x, y + h / 2 + 15);
}

function drawProducts() {
    products.forEach((p) => {
        let x: number, y: number;

        if (p.in_machine_id > 0) {
            // Product is in a machine
            const machine = machines.get(p.in_machine_id);
            if (!machine) return;
            const pos = gridToIso(machine.grid_x, machine.grid_y);
            x = pos.x;
            y = pos.y - 50; // Above machine
        } else if (p.on_conveyor_id > 0) {
            // Product is on a conveyor
            const conveyor = conveyors.get(p.on_conveyor_id);
            if (!conveyor) return;
            const start = gridToIso(conveyor.start_x, conveyor.start_y);
            const end = gridToIso(conveyor.end_x, conveyor.end_y);
            const t = p.conveyor_position;
            x = start.x + (end.x - start.x) * t;
            y = start.y + (end.y - start.y) * t - 10; // Slightly above conveyor
        } else {
            return;
        }

        // Product appearance based on type
        const colors = ['#9ca3af', '#60a5fa', '#f59e0b', '#10b981', '#8b5cf6', '#ec4899', '#06b6d4'];
        const color = colors[p.product_type] || '#fbbf24';

        // Draw product as a small box
        ctx.fillStyle = color;
        ctx.beginPath();
        ctx.arc(x, y, 6, 0, Math.PI * 2);
        ctx.fill();

        // Quality indicator (darker = lower quality)
        if (p.quality_pct < 90) {
            ctx.strokeStyle = p.quality_pct < 80 ? '#ff4466' : '#ffaa00';
            ctx.lineWidth = 2;
            ctx.stroke();
        }
    });
}

// Demo mode
function startDemoMode() {
    // Demo machines
    const demoMachines: MachineJson[] = [
        { machine_id: 1, name: 'CNC-01', machine_type: 0, state: 1, grid_x: 0, grid_y: 0, temperature_c: 45.2, vibration_mm_s: 1.2, power_kw: 5.5, efficiency_pct: 92, items_processed: 127, items_rejected: 3, cycle_time_s: 2.0, timestamp_us: 0 },
        { machine_id: 2, name: 'WELD-01', machine_type: 1, state: 0, grid_x: 2, grid_y: 0, temperature_c: 28.1, vibration_mm_s: 0.1, power_kw: 0.5, efficiency_pct: 95, items_processed: 124, items_rejected: 2, cycle_time_s: 2.0, timestamp_us: 0 },
        { machine_id: 3, name: 'ASSY-01', machine_type: 2, state: 1, grid_x: 4, grid_y: 0, temperature_c: 42.8, vibration_mm_s: 0.8, power_kw: 4.2, efficiency_pct: 88, items_processed: 120, items_rejected: 5, cycle_time_s: 2.0, timestamp_us: 0 },
        { machine_id: 4, name: 'PAINT-01', machine_type: 3, state: 1, grid_x: 4, grid_y: 2, temperature_c: 38.5, vibration_mm_s: 0.3, power_kw: 3.8, efficiency_pct: 94, items_processed: 118, items_rejected: 1, cycle_time_s: 2.0, timestamp_us: 0 },
        { machine_id: 5, name: 'QC-01', machine_type: 4, state: 0, grid_x: 2, grid_y: 2, temperature_c: 25.0, vibration_mm_s: 0.05, power_kw: 0.8, efficiency_pct: 99, items_processed: 115, items_rejected: 8, cycle_time_s: 2.0, timestamp_us: 0 },
        { machine_id: 6, name: 'PACK-01', machine_type: 5, state: 5, grid_x: 0, grid_y: 2, temperature_c: 55.0, vibration_mm_s: 3.2, power_kw: 0.0, efficiency_pct: 0, items_processed: 100, items_rejected: 0, cycle_time_s: 2.0, timestamp_us: 0 },
    ];
    demoMachines.forEach(m => machines.set(m.machine_id, m));
    updateMachinesList();

    // Demo conveyors
    const demoConveyors: ConveyorJson[] = [
        { conveyor_id: 1, state: 0, start_x: 0, start_y: 0, end_x: 2, end_y: 0, speed_m_s: 0.3, capacity: 5, current_items: 1, timestamp_us: 0 },
        { conveyor_id: 2, state: 0, start_x: 2, start_y: 0, end_x: 4, end_y: 0, speed_m_s: 0.3, capacity: 5, current_items: 0, timestamp_us: 0 },
        { conveyor_id: 3, state: 0, start_x: 4, start_y: 0, end_x: 4, end_y: 2, speed_m_s: 0.3, capacity: 5, current_items: 2, timestamp_us: 0 },
        { conveyor_id: 4, state: 0, start_x: 4, start_y: 2, end_x: 2, end_y: 2, speed_m_s: 0.3, capacity: 5, current_items: 1, timestamp_us: 0 },
        { conveyor_id: 5, state: 0, start_x: 2, start_y: 2, end_x: 0, end_y: 2, speed_m_s: 0.3, capacity: 5, current_items: 0, timestamp_us: 0 },
    ];
    demoConveyors.forEach(c => conveyors.set(c.conveyor_id, c));

    // Demo products
    const demoProducts: ProductJson[] = [
        { product_id: 1, product_type: 1, quality_pct: 100, on_conveyor_id: 1, in_machine_id: 0, conveyor_position: 0.5, batch_id: 1, timestamp_us: 0 },
        { product_id: 2, product_type: 2, quality_pct: 95, on_conveyor_id: 3, in_machine_id: 0, conveyor_position: 0.3, batch_id: 1, timestamp_us: 0 },
        { product_id: 3, product_type: 3, quality_pct: 85, on_conveyor_id: 3, in_machine_id: 0, conveyor_position: 0.7, batch_id: 1, timestamp_us: 0 },
        { product_id: 4, product_type: 4, quality_pct: 78, on_conveyor_id: 4, in_machine_id: 0, conveyor_position: 0.4, batch_id: 1, timestamp_us: 0 },
    ];
    demoProducts.forEach(p => products.set(p.product_id, p));

    // Demo alert
    alerts.set(1, { alert_id: 1, machine_id: 6, severity: 2, message: 'PACK-01 fault detected!', acknowledged: false, timestamp_us: 0 });
    updateAlertsList();

    // Demo factory state
    factoryState = {
        machines_total: 6,
        machines_working: 3,
        machines_idle: 2,
        machines_fault: 1,
        products_in_progress: 4,
        products_completed: 100,
        products_rejected: 12,
        throughput_per_hour: 45.2,
        overall_efficiency_pct: 50,
        total_power_kw: 14.8,
        active_alerts: 1,
        timestamp_us: 0,
    };
    updateFactoryStats();

    // Animate demo
    setInterval(() => {
        // Move products
        demoProducts.forEach(p => {
            if (p.on_conveyor_id > 0) {
                p.conveyor_position += 0.01;
                if (p.conveyor_position > 1) p.conveyor_position = 0;
                products.set(p.product_id, { ...p });
            }
        });

        // Toggle machine states randomly
        if (Math.random() < 0.05) {
            const mid = Math.floor(Math.random() * 5) + 1;
            const m = machines.get(mid);
            if (m && m.state !== 5) {
                m.state = m.state === 1 ? 0 : 1;
                m.temperature_c = m.state === 1 ? 40 + Math.random() * 10 : 25 + Math.random() * 5;
                m.power_kw = m.state === 1 ? 4 + Math.random() * 3 : 0.5;
                machines.set(mid, { ...m });
                updateMachinesList();
            }
        }
    }, 50);
}

// Start
document.addEventListener('DOMContentLoaded', init);
