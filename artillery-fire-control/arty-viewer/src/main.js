// Artillery Fire Control - Tactical Map Viewer
//
// Connects to hdds-ws WebSocket bridge on port 9090 and renders
// a real-time top-down tactical map with batteries, targets,
// trajectories, and rounds in flight.
//
// CONCEPT DEMO - Simulated data for showcase purposes

// ---------------------------------------------------------------------------
//  State
// ---------------------------------------------------------------------------

const state = {
  batteries: new Map(),   // battery_id -> BatteryStatus
  missions: new Map(),    // mission_id -> FireMission
  solutions: new Map(),   // mission_id -> BallisticSolution
  rounds: new Map(),      // round_id -> RoundInFlight
  impacts: [],            // ImpactObservation[]
  meteo: null,            // MeteoData
  impactMarkers: [],      // { x, y, age, destroyed }
  trajectories: [],       // { fromX, fromY, toX, toY, missionId, age }
};

// Map dimensions (meters)
const MAP_W = 20000;
const MAP_H = 20000;

// Canvas
let canvas, ctx;
let canvasW, canvasH;

// WebSocket
let ws = null;
let wsConnected = false;

// ---------------------------------------------------------------------------
//  WebSocket connection
// ---------------------------------------------------------------------------

function connectWs() {
  const host = window.location.hostname || 'localhost';
  const url = `ws://${host}:9090/ws`;
  ws = new WebSocket(url);

  ws.onopen = () => {
    wsConnected = true;
    updateWsIndicator();
    console.log('[WS] Connected to', url);

    // Subscribe to all artillery topics
    const topics = [
      'arty/battery',
      'arty/mission',
      'arty/solution',
      'arty/round',
      'arty/impact',
      'arty/meteo',
    ];
    topics.forEach(topic => {
      ws.send(JSON.stringify({ type: 'subscribe', topic }));
    });
  };

  ws.onmessage = (evt) => {
    try {
      const msg = JSON.parse(evt.data);
      window._msgCount = (window._msgCount || 0) + 1;
      if (window._msgCount <= 20) {
        console.log(`[WS] msg #${window._msgCount} type=${msg.type} topic=${msg.topic}`, JSON.stringify(msg).slice(0, 300));
      }
      handleMessage(msg);
    } catch (e) {
      console.warn('[WS] Parse error:', e);
    }
  };

  ws.onclose = () => {
    wsConnected = false;
    updateWsIndicator();
    console.log('[WS] Disconnected, reconnecting in 3s...');
    setTimeout(connectWs, 3000);
  };

  ws.onerror = (err) => {
    console.error('[WS] Error:', err);
  };
}

function updateWsIndicator() {
  const el = document.getElementById('ws-indicator');
  if (wsConnected) {
    el.textContent = 'CONNECTED';
    el.className = 'ws-status ws-connected';
  } else {
    el.textContent = 'DISCONNECTED';
    el.className = 'ws-status ws-disconnected';
  }
}

// ---------------------------------------------------------------------------
//  Message handling
// ---------------------------------------------------------------------------

function handleMessage(msg) {
  if (msg.type !== 'data') return;
  const topic = msg.topic;
  const data = msg.sample;

  if (!topic || !data) return;

  switch (topic) {
    case 'arty/battery':
      state.batteries.set(data.battery_id, data);
      updateBatteryPanel();
      break;

    case 'arty/mission':
      state.missions.set(data.mission_id, data);
      updateMissionPanel();
      break;

    case 'arty/solution': {
      state.solutions.set(data.mission_id, data);
      // Record trajectory line
      const bat = state.batteries.get(data.battery_id);
      const mission = state.missions.get(data.mission_id);
      if (bat && mission) {
        state.trajectories.push({
          fromX: bat.lat_deg,  // sim uses lat_deg as X
          fromY: bat.lon_deg,  // sim uses lon_deg as Y
          toX: mission.target_lat,
          toY: mission.target_lon,
          missionId: data.mission_id,
          age: 0,
        });
      }
      break;
    }

    case 'arty/round':
      state.rounds.set(data.round_id, {
        ...data,
        lastUpdate: performance.now(),
      });
      break;

    case 'arty/impact':
      state.impacts.push(data);
      state.impactMarkers.push({
        x: data.impact_lat,
        y: data.impact_lon,
        age: 0,
        destroyed: data.target_destroyed,
      });
      updateImpactLog(data);
      if (state.impacts.length > 50) {
        state.impacts.shift();
      }
      break;

    case 'arty/meteo':
      state.meteo = data;
      updateMeteoPanel();
      break;
  }
}

// ---------------------------------------------------------------------------
//  UI panel updates
// ---------------------------------------------------------------------------

function updateBatteryPanel() {
  const el = document.getElementById('battery-panel');
  let html = '';
  for (const [id, bat] of state.batteries) {
    const stateColor = bat.state === 0 ? '#33ff33'
      : bat.state === 2 ? '#ff3333'
      : bat.state === 3 ? '#ffa500'
      : '#888888';
    const stateName = ['Ready', 'Loading', 'Firing', 'Cooldown', 'Offline'][bat.state] || '??';
    html += `<div class="battery-row">
      <span class="name">${id}</span>
      <span style="color:${stateColor}">${stateName}</span>
      <span>${bat.rounds_remaining} rds</span>
      <span>${bat.barrel_temp_c.toFixed(0)}C</span>
    </div>`;
  }
  el.innerHTML = html;
}

function updateMeteoPanel() {
  const el = document.getElementById('meteo-panel');
  const m = state.meteo;
  if (!m) return;
  el.innerHTML = `
    Wind: ${m.wind_speed_mps.toFixed(1)} m/s @ ${m.wind_dir_deg.toFixed(0)}&deg;<br>
    Temp: ${m.temperature_c.toFixed(1)}&deg;C | Press: ${m.pressure_hpa.toFixed(0)} hPa<br>
    Humidity: ${m.humidity_pct.toFixed(0)}% | Density: ${m.air_density.toFixed(3)} kg/m3
  `;
}

function updateMissionPanel() {
  const el = document.getElementById('mission-panel');
  let html = '';
  // Show most recent missions first
  const missions = [...state.missions.values()].reverse().slice(0, 8);
  for (const m of missions) {
    const statusName = ['Pending', 'Computing', 'Firing', 'Adjusting', 'Complete', 'Cancelled'][m.status] || '??';
    const statusClass = ['pending', 'computing', 'firing', 'adjusting', 'complete', 'cancelled'][m.status] || '';
    const active = m.status !== 4 && m.status !== 5;
    const roundName = ['HE', 'Smoke', 'Illum', 'WP'][m.round_type] || '??';
    html += `<div class="mission-entry ${active ? 'active' : 'complete'}">
      <span class="status-badge status-${statusClass}">${statusName}</span>
      <strong> #${m.mission_id}</strong> ${roundName} x${m.rounds_requested}<br>
      <span style="color:#888">${m.target_description || ''}</span><br>
      <span style="color:#666">from ${m.requesting_unit || '??'}</span>
    </div>`;
  }
  el.innerHTML = html || '<div style="color:#555">No missions yet</div>';
}

function updateImpactLog(impact) {
  const el = document.getElementById('impact-log');
  const div = document.createElement('div');
  const color = impact.target_destroyed ? '#33ff33' : '#ffa500';
  div.innerHTML = `<span style="color:${color}">M#${impact.mission_id}</span> `
    + `err: ${impact.error_range_m.toFixed(0)}m R / ${impact.error_deflection_m.toFixed(0)}m D `
    + `${impact.target_destroyed ? '<b style="color:#33ff33">DESTROYED</b>' : impact.correction}`;
  el.insertBefore(div, el.firstChild);
  // Keep max 20 entries
  while (el.children.length > 20) {
    el.removeChild(el.lastChild);
  }
}

// ---------------------------------------------------------------------------
//  Canvas rendering
// ---------------------------------------------------------------------------

function mapToCanvas(x, y) {
  // x,y in meters -> canvas pixels
  // Y is flipped: map Y=0 at bottom, canvas Y=0 at top
  const px = (x / MAP_W) * canvasW;
  const py = canvasH - (y / MAP_H) * canvasH;
  return [px, py];
}

function drawGrid() {
  ctx.strokeStyle = '#0d1f0d';
  ctx.lineWidth = 1;

  // Grid every 2km
  const step = 2000;
  for (let x = 0; x <= MAP_W; x += step) {
    const [px] = mapToCanvas(x, 0);
    ctx.beginPath();
    ctx.moveTo(px, 0);
    ctx.lineTo(px, canvasH);
    ctx.stroke();
  }
  for (let y = 0; y <= MAP_H; y += step) {
    const [, py] = mapToCanvas(0, y);
    ctx.beginPath();
    ctx.moveTo(0, py);
    ctx.lineTo(canvasW, py);
    ctx.stroke();
  }

  // Grid labels every 4km
  ctx.fillStyle = '#1a3a1a';
  ctx.font = '10px Courier New';
  for (let x = 0; x <= MAP_W; x += 4000) {
    const [px] = mapToCanvas(x, 0);
    ctx.fillText(`${(x / 1000).toFixed(0)}km`, px + 2, canvasH - 4);
  }
  for (let y = 0; y <= MAP_H; y += 4000) {
    const [, py] = mapToCanvas(0, y);
    ctx.fillText(`${(y / 1000).toFixed(0)}km`, 4, py - 4);
  }
}

function drawBattery(x, y, id, batteryState) {
  const [px, py] = mapToCanvas(x, y);
  const size = 10;

  // Triangle pointing up (friendly)
  ctx.beginPath();
  ctx.moveTo(px, py - size);
  ctx.lineTo(px - size * 0.8, py + size * 0.6);
  ctx.lineTo(px + size * 0.8, py + size * 0.6);
  ctx.closePath();

  // Color based on state
  const stateVal = typeof batteryState === 'number' ? batteryState : 0;
  if (stateVal === 2) { // Firing
    ctx.fillStyle = '#ff4444';
    ctx.strokeStyle = '#ff8888';
  } else if (stateVal === 3) { // Cooldown
    ctx.fillStyle = '#cc8800';
    ctx.strokeStyle = '#ffaa44';
  } else {
    ctx.fillStyle = '#ffa500';
    ctx.strokeStyle = '#ffcc66';
  }

  ctx.lineWidth = 2;
  ctx.fill();
  ctx.stroke();

  // Label
  ctx.fillStyle = '#ffa500';
  ctx.font = 'bold 11px Courier New';
  ctx.textAlign = 'center';
  ctx.fillText(id, px, py + size + 14);
  ctx.textAlign = 'left';
}

function drawTarget(x, y, label) {
  const [px, py] = mapToCanvas(x, y);
  const size = 8;

  // Crosshair
  ctx.strokeStyle = '#ff3333';
  ctx.lineWidth = 1.5;

  ctx.beginPath();
  ctx.moveTo(px - size, py);
  ctx.lineTo(px + size, py);
  ctx.moveTo(px, py - size);
  ctx.lineTo(px, py + size);
  ctx.stroke();

  // Circle
  ctx.beginPath();
  ctx.arc(px, py, size * 0.7, 0, Math.PI * 2);
  ctx.stroke();

  // Label
  if (label) {
    ctx.fillStyle = '#ff3333';
    ctx.font = '9px Courier New';
    ctx.textAlign = 'center';
    ctx.fillText(label, px, py - size - 4);
    ctx.textAlign = 'left';
  }
}

function drawTrajectory(fromX, fromY, toX, toY, alpha) {
  const [fx, fy] = mapToCanvas(fromX, fromY);
  const [tx, ty] = mapToCanvas(toX, toY);

  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.strokeStyle = '#555500';
  ctx.lineWidth = 1;
  ctx.setLineDash([4, 6]);

  ctx.beginPath();
  ctx.moveTo(fx, fy);
  ctx.lineTo(tx, ty);
  ctx.stroke();

  ctx.setLineDash([]);
  ctx.restore();
}

function drawRound(x, y, alt) {
  const [px, py] = mapToCanvas(x, y);

  // Size based on altitude (bigger = higher)
  const baseSize = 3;
  const altFactor = Math.min(alt / 5000, 1);
  const size = baseSize + altFactor * 4;

  // Glow effect
  ctx.beginPath();
  ctx.arc(px, py, size + 3, 0, Math.PI * 2);
  ctx.fillStyle = 'rgba(255, 255, 0, 0.15)';
  ctx.fill();

  // Core dot
  ctx.beginPath();
  ctx.arc(px, py, size, 0, Math.PI * 2);
  ctx.fillStyle = '#ffff00';
  ctx.fill();

  // Altitude indicator text
  if (alt > 100) {
    ctx.fillStyle = '#aaaa00';
    ctx.font = '8px Courier New';
    ctx.fillText(`${(alt / 1000).toFixed(1)}k`, px + size + 3, py - 2);
  }
}

function drawImpactMarker(x, y, age, destroyed) {
  const [px, py] = mapToCanvas(x, y);
  const fade = Math.max(0, 1 - age / 300); // fade over 5 seconds (300 frames)

  if (fade <= 0) return false; // expired

  ctx.save();
  ctx.globalAlpha = fade;

  // Outer ring (expanding)
  const ringSize = 8 + age * 0.15;
  ctx.beginPath();
  ctx.arc(px, py, ringSize, 0, Math.PI * 2);
  ctx.strokeStyle = destroyed ? '#33ff33' : '#ff6600';
  ctx.lineWidth = 2;
  ctx.stroke();

  // Inner splash
  ctx.beginPath();
  ctx.arc(px, py, 4, 0, Math.PI * 2);
  ctx.fillStyle = destroyed ? '#33ff33' : '#ff3300';
  ctx.fill();

  // "X" for destroyed
  if (destroyed) {
    ctx.strokeStyle = '#33ff33';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(px - 6, py - 6);
    ctx.lineTo(px + 6, py + 6);
    ctx.moveTo(px + 6, py - 6);
    ctx.lineTo(px - 6, py + 6);
    ctx.stroke();
  }

  ctx.restore();
  return true; // still visible
}

function drawWindIndicator() {
  if (!state.meteo) return;

  const cx = canvasW - 50;
  const cy = 50;
  const radius = 25;

  // Circle
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.strokeStyle = '#1a3a1a';
  ctx.lineWidth = 1;
  ctx.stroke();

  // Wind direction arrow
  const dirRad = (state.meteo.wind_dir_deg * Math.PI) / 180;
  const arrowLen = Math.min(state.meteo.wind_speed_mps * 2, radius - 2);

  const tipX = cx + Math.sin(dirRad) * arrowLen;
  const tipY = cy - Math.cos(dirRad) * arrowLen;

  ctx.beginPath();
  ctx.moveTo(cx, cy);
  ctx.lineTo(tipX, tipY);
  ctx.strokeStyle = '#44aaff';
  ctx.lineWidth = 2;
  ctx.stroke();

  // Arrowhead
  const headLen = 6;
  const headAngle = Math.atan2(tipY - cy, tipX - cx);
  ctx.beginPath();
  ctx.moveTo(tipX, tipY);
  ctx.lineTo(
    tipX - headLen * Math.cos(headAngle - 0.4),
    tipY - headLen * Math.sin(headAngle - 0.4)
  );
  ctx.moveTo(tipX, tipY);
  ctx.lineTo(
    tipX - headLen * Math.cos(headAngle + 0.4),
    tipY - headLen * Math.sin(headAngle + 0.4)
  );
  ctx.stroke();

  // Label
  ctx.fillStyle = '#44aaff';
  ctx.font = '9px Courier New';
  ctx.textAlign = 'center';
  ctx.fillText('WIND', cx, cy + radius + 12);
  ctx.fillText(`${state.meteo.wind_speed_mps.toFixed(1)}m/s`, cx, cy + radius + 22);
  ctx.textAlign = 'left';
}

function drawTitle() {
  ctx.fillStyle = '#1a3a1a';
  ctx.font = 'bold 14px Courier New';
  ctx.fillText('HDDS ARTILLERY FIRE CONTROL', 10, 20);

  ctx.fillStyle = '#0f250f';
  ctx.font = '10px Courier New';
  ctx.fillText('TACTICAL MAP - 20km x 20km', 10, 34);
}

function render() {
  // Resize canvas if needed
  const container = document.getElementById('map-container');
  const dpr = window.devicePixelRatio || 1;
  const rect = container.getBoundingClientRect();
  if (canvas.width !== rect.width * dpr || canvas.height !== rect.height * dpr) {
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);
  }
  canvasW = rect.width;
  canvasH = rect.height;

  // Clear
  ctx.fillStyle = '#0d1a0d';
  ctx.fillRect(0, 0, canvasW, canvasH);

  // Grid
  drawGrid();

  // Title
  drawTitle();

  // Wind indicator
  drawWindIndicator();

  // Trajectory lines (fade over time)
  for (let i = state.trajectories.length - 1; i >= 0; i--) {
    const t = state.trajectories[i];
    t.age++;
    const alpha = Math.max(0, 1 - t.age / 600); // fade over 10s
    if (alpha <= 0) {
      state.trajectories.splice(i, 1);
      continue;
    }
    drawTrajectory(t.fromX, t.fromY, t.toX, t.toY, alpha);
  }

  // Targets from active missions
  for (const [, m] of state.missions) {
    if (m.status === 4 || m.status === 5) continue; // skip complete/cancelled
    drawTarget(m.target_lat, m.target_lon, m.target_description);
  }

  // Impact markers
  for (let i = state.impactMarkers.length - 1; i >= 0; i--) {
    const im = state.impactMarkers[i];
    im.age++;
    const visible = drawImpactMarker(im.x, im.y, im.age, im.destroyed);
    if (!visible) {
      state.impactMarkers.splice(i, 1);
    }
  }

  // Rounds in flight
  const now = performance.now();
  for (const [rid, r] of state.rounds) {
    // Remove stale rounds (not updated in 2s)
    if (now - r.lastUpdate > 2000) {
      state.rounds.delete(rid);
      continue;
    }
    drawRound(r.lat_deg, r.lon_deg, r.alt_m);
  }

  // Batteries (drawn last so they're on top)
  for (const [, bat] of state.batteries) {
    drawBattery(bat.lat_deg, bat.lon_deg, bat.battery_id, bat.state);
  }

  // FEBA line (Forward Edge of Battle Area) -- decorative
  ctx.save();
  ctx.strokeStyle = '#331111';
  ctx.lineWidth = 1;
  ctx.setLineDash([8, 4]);
  const [, febaY] = mapToCanvas(0, 10000);
  ctx.beginPath();
  ctx.moveTo(0, febaY);
  ctx.lineTo(canvasW, febaY);
  ctx.stroke();
  ctx.setLineDash([]);
  ctx.fillStyle = '#331111';
  ctx.font = '9px Courier New';
  ctx.fillText('--- FEBA ---', canvasW / 2 - 30, febaY - 4);
  ctx.restore();

  requestAnimationFrame(render);
}

// ---------------------------------------------------------------------------
//  Init
// ---------------------------------------------------------------------------

function init() {
  canvas = document.getElementById('tactical-map');
  ctx = canvas.getContext('2d');

  connectWs();
  requestAnimationFrame(render);
}

document.addEventListener('DOMContentLoaded', init);
