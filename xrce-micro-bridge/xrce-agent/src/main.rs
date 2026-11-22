// CONCEPT DEMO - Simulated data for showcase purposes
//
// XRCE Agent Monitor
//
// This demo showcases how an hdds-xrce agent bridges resource-constrained
// micro-clients (ESP32, STM32) into the full DDS domain. The agent:
//
//   1. Manages XRCE client sessions (CREATE_CLIENT, session table)
//   2. Bridges sensor data from micro-clients to DDS writers
//   3. Bridges actuator commands from DDS readers back to clients
//   4. Displays a live ratatui TUI with session table, data flow, and stats
//
// In this demo the XRCE protocol layer is real (hdds-xrce types + agent),
// but the physical transport is simulated: sensor data arrives via the
// standard DDS topic "xrce/sensors" published by the client-sim binary
// rather than over a true UDP/Serial XRCE transport.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};

use hdds::{Participant, QoS, TransportMode};
use hdds_xrce::{XrceAgent, XrceAgentConfig, NullBridge};
use xrce_types::{ActuatorCommand, MicroSensorData, XrceSessionInfo};

use clap::Parser;

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(name = "xrce-agent", about = "HDDS XRCE Agent with TUI monitor")]
struct Cli {
    /// DDS domain ID
    #[arg(long, default_value_t = DOMAIN_ID)]
    domain_id: u32,

    /// Maximum XRCE clients
    #[arg(long, default_value_t = 128)]
    max_clients: usize,

    /// TUI refresh rate in milliseconds
    #[arg(long, default_value_t = 100)]
    refresh_ms: u64,
}

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

/// Tracks a single simulated XRCE client session.
#[derive(Debug, Clone)]
struct ClientEntry {
    client_id: String,
    transport: String,
    connected: bool,
    reliable_streams: i32,
    best_effort_streams: i32,
    bytes_in: i64,
    bytes_out: i64,
    last_seen: Instant,
}

/// A data flow event displayed in the live feed.
#[derive(Debug, Clone)]
struct DataFlowEvent {
    timestamp: Instant,
    #[allow(dead_code)]
    topic: String,
    client_id: String,
    sensor_type: String,
    value: String,
}

/// A pending actuator command.
#[derive(Debug, Clone)]
struct PendingCommand {
    target: String,
    command_type: String,
    value: f64,
    timestamp: Instant,
}

/// The shared state updated by the DDS reader thread and rendered by the TUI.
struct AppState {
    clients: Vec<ClientEntry>,
    data_flow: VecDeque<DataFlowEvent>,
    actuator_queue: VecDeque<PendingCommand>,
    total_samples: u64,
    total_bytes: u64,
    start_time: Instant,
    #[allow(dead_code)]
    xrce_agent_sessions: usize,
}

impl AppState {
    fn new() -> Self {
        Self {
            clients: Vec::new(),
            data_flow: VecDeque::with_capacity(64),
            actuator_queue: VecDeque::with_capacity(32),
            total_samples: 0,
            total_bytes: 0,
            start_time: Instant::now(),
            xrce_agent_sessions: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// DDS reader thread
// ---------------------------------------------------------------------------

/// Background task: reads MicroSensorData from DDS and updates AppState.
fn dds_reader_loop(
    participant: Arc<Participant>,
    state: Arc<Mutex<AppState>>,
) {
    let reader = match participant.topic::<MicroSensorData>("xrce/sensors")
        .and_then(|t| t.reader().qos(QoS::default()).build()) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to create sensor reader: {}", e);
            return;
        }
    };

    let cmd_writer = match participant.topic::<ActuatorCommand>("xrce/actuators")
        .and_then(|t| t.writer().qos(QoS::default()).build()) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to create actuator writer: {}", e);
            return;
        }
    };

    let session_writer = match participant.topic::<XrceSessionInfo>("xrce/sessions")
        .and_then(|t| t.writer().qos(QoS::default()).build()) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to create session writer: {}", e);
            return;
        }
    };

    // Periodic actuator command injection (demo: every 10s toggle an LED)
    let mut last_cmd_time = Instant::now();
    let cmd_targets = [
        "client-temp-01",
        "client-humid-01",
        "client-press-01",
        "client-light-01",
    ];
    let mut cmd_idx = 0usize;

    loop {
        // Poll sensor data
        match reader.take() {
            Ok(Some(sample)) => {
                let estimated_size = sample.client_id.len()
                    + sample.sensor_type.len()
                    + sample.unit.len()
                    + 8 + 4 + 4 + 8; // f64 + f32 + i32 + i64

                let mut st = state.lock().unwrap();
                st.total_samples += 1;
                st.total_bytes += estimated_size as u64;

                // Update or insert client entry
                let now = Instant::now();
                if let Some(entry) = st.clients.iter_mut().find(|c| c.client_id == sample.client_id)
                {
                    entry.bytes_in += estimated_size as i64;
                    entry.last_seen = now;
                    entry.connected = true;
                } else {
                    st.clients.push(ClientEntry {
                        client_id: sample.client_id.clone(),
                        transport: "udp".to_string(),
                        connected: true,
                        reliable_streams: 1,
                        best_effort_streams: 1,
                        bytes_in: estimated_size as i64,
                        bytes_out: 0,
                        last_seen: now,
                    });
                }

                // Add to data flow (keep last 20 entries)
                let event = DataFlowEvent {
                    timestamp: now,
                    topic: "xrce/sensors".to_string(),
                    client_id: sample.client_id.clone(),
                    sensor_type: sample.sensor_type.clone(),
                    value: format!("{:.2} {}", sample.value, sample.unit),
                };
                st.data_flow.push_front(event);
                if st.data_flow.len() > 20 {
                    st.data_flow.pop_back();
                }

                // Publish session info for this client
                let session_info = XrceSessionInfo {
                    client_key: format!(
                        "{:08X}",
                        u32::from_le_bytes(
                            sample.client_id.as_bytes()[..4]
                                .try_into()
                                .unwrap_or([0; 4])
                        )
                    ),
                    session_id: st
                        .clients
                        .iter()
                        .position(|c| c.client_id == sample.client_id)
                        .map(|i| i as i32 + 1)
                        .unwrap_or(0),
                    transport: "udp".to_string(),
                    connected: true,
                    reliable_streams: 1,
                    best_effort_streams: 1,
                    bytes_received: st
                        .clients
                        .iter()
                        .find(|c| c.client_id == sample.client_id)
                        .map(|c| c.bytes_in)
                        .unwrap_or(0),
                    bytes_sent: 0,
                    timestamp_ns: sample.timestamp_ns,
                };
                let _ = session_writer.write(&session_info);
            }
            Ok(None) => {
                // No data available
            }
            Err(e) => {
                tracing::warn!("Reader error: {}", e);
            }
        }

        // Mark clients as disconnected if not seen for 5 seconds
        {
            let mut st = state.lock().unwrap();
            let now = Instant::now();
            for client in st.clients.iter_mut() {
                if now.duration_since(client.last_seen) > Duration::from_secs(5) {
                    client.connected = false;
                }
            }
        }

        // Inject a demo actuator command every 10 seconds
        if last_cmd_time.elapsed() > Duration::from_secs(10) {
            last_cmd_time = Instant::now();
            let target = cmd_targets[cmd_idx % cmd_targets.len()];
            cmd_idx += 1;

            let cmd = ActuatorCommand {
                target_client: target.to_string(),
                command_type: "led_set".to_string(),
                value: if cmd_idx % 2 == 0 { 1.0 } else { 0.0 },
                timestamp_ns: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as i64,
            };

            let _ = cmd_writer.write(&cmd);

            let mut st = state.lock().unwrap();
            st.actuator_queue.push_front(PendingCommand {
                target: target.to_string(),
                command_type: "led_set".to_string(),
                value: cmd.value,
                timestamp: Instant::now(),
            });
            if st.actuator_queue.len() > 10 {
                st.actuator_queue.pop_back();
            }

            // Update bytes_out for the target client
            if let Some(entry) = st.clients.iter_mut().find(|c| c.client_id == target) {
                entry.bytes_out += 32;
            }
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing (logs go to file so they don't interfere with TUI)
    tracing_subscriber::fmt()
        .with_env_filter("xrce_agent=debug,hdds=warn")
        .with_writer(std::io::stderr)
        .init();

    // Create the real XRCE agent instance (demonstrates the hdds-xrce API).
    // The agent refuses to start with neither an accepted client key nor an
    // explicit open admission, which is the right default for a deployment and
    // made this binary panic on startup. No XRCE client connects in this demo:
    // the dashboard reads DDS samples published by xrce-client-sim. A real
    // deployment lists the keys of its micro-clients in accepted_client_keys
    // instead of opening admission.
    let xrce_config = XrceAgentConfig {
        udp_port: 2019,
        max_clients: cli.max_clients,
        session_timeout_ms: 30_000,
        heartbeat_period_ms: 200,
        max_message_size: 512,
        open_admission: true,
        ..XrceAgentConfig::default()
    };

    let bridge = Arc::new(NullBridge);
    let _xrce_agent = XrceAgent::new(xrce_config, bridge)
        .expect("Failed to create XRCE agent");

    tracing::info!(
        "XRCE Agent created (max_clients={}, session_timeout=30s)",
        cli.max_clients
    );

    // Create DDS participant for bridging (build() returns Arc<Participant>)
    let participant = Participant::builder("xrce-agent")
        .domain_id(cli.domain_id)
        .with_transport(TransportMode::UdpMulticast)
        .build()
        .expect("Failed to create DDS participant");

    tracing::info!(
        "DDS participant created on domain {}",
        cli.domain_id
    );

    // Shared state
    let state = Arc::new(Mutex::new(AppState::new()));

    // Spawn DDS reader thread
    let reader_state = Arc::clone(&state);
    let reader_participant = Arc::clone(&participant);
    std::thread::spawn(move || {
        dds_reader_loop(reader_participant, reader_state);
    });

    // Run TUI
    run_tui(state, cli.refresh_ms)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// TUI rendering
// ---------------------------------------------------------------------------

fn run_tui(state: Arc<Mutex<AppState>>, refresh_ms: u64) -> io::Result<()> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let tick_rate = Duration::from_millis(refresh_ms);

    loop {
        terminal.draw(|frame| {
            let st = state.lock().unwrap();
            draw_ui(frame, &st);
        })?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press
                    && (key.code == KeyCode::Char('q') || key.code == KeyCode::Esc)
                {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn draw_ui(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    // Top-level layout: header (3 rows) + body
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header / stats bar
            Constraint::Min(10),   // Body
        ])
        .split(area);

    // Header: Agent stats
    draw_header(frame, outer[0], state);

    // Body: left (clients + actuator queue) | right (data flow)
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
        ])
        .split(outer[1]);

    // Left: clients table + actuator queue
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(65),
            Constraint::Percentage(35),
        ])
        .split(body[0]);

    draw_clients_table(frame, left[0], state);
    draw_actuator_queue(frame, left[1], state);

    // Right: data flow
    draw_data_flow(frame, body[1], state);
}

fn draw_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let uptime = state.start_time.elapsed();
    let uptime_str = format!(
        "{:02}:{:02}:{:02}",
        uptime.as_secs() / 3600,
        (uptime.as_secs() % 3600) / 60,
        uptime.as_secs() % 60,
    );

    let connected = state.clients.iter().filter(|c| c.connected).count();
    let total_clients = state.clients.len();

    let throughput = if uptime.as_secs() > 0 {
        state.total_bytes / uptime.as_secs()
    } else {
        0
    };

    let header_text = format!(
        " HDDS XRCE Agent Monitor  |  Clients: {}/{}  |  Samples: {}  |  Throughput: {} B/s  |  Uptime: {}  |  Press 'q' to quit",
        connected, total_clients, state.total_samples, throughput, uptime_str,
    );

    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" XRCE Agent Stats ")
                .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        );
    frame.render_widget(header, area);
}

fn draw_clients_table(frame: &mut Frame, area: Rect, state: &AppState) {
    let header_cells = [
        "Client ID",
        "Transport",
        "Status",
        "Reliable",
        "BestEffort",
        "Bytes In",
        "Bytes Out",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));

    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = state
        .clients
        .iter()
        .map(|c| {
            let status_style = if c.connected {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            };
            let status_text = if c.connected { "ONLINE" } else { "OFFLINE" };

            Row::new(vec![
                Cell::from(c.client_id.clone()),
                Cell::from(c.transport.clone()),
                Cell::from(status_text).style(status_style),
                Cell::from(c.reliable_streams.to_string()),
                Cell::from(c.best_effort_streams.to_string()),
                Cell::from(format_bytes(c.bytes_in)),
                Cell::from(format_bytes(c.bytes_out)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Min(18),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Client Sessions ")
                .title_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(table, area);
}

fn draw_data_flow(frame: &mut Frame, area: Rect, state: &AppState) {
    let lines: Vec<Line> = state
        .data_flow
        .iter()
        .map(|evt| {
            let age = evt.timestamp.elapsed();
            let age_str = if age.as_secs() > 0 {
                format!("{}s ago", age.as_secs())
            } else {
                format!("{}ms ago", age.as_millis())
            };

            let sensor_color = match evt.sensor_type.as_str() {
                "temperature" => Color::Red,
                "humidity" => Color::Blue,
                "pressure" => Color::Magenta,
                "light" => Color::Yellow,
                _ => Color::White,
            };

            Line::from(vec![
                Span::styled(
                    format!("[{}] ", age_str),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{:<12}", evt.sensor_type),
                    Style::default().fg(sensor_color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    evt.value.clone(),
                    Style::default().fg(Color::White),
                ),
                Span::raw(" <- "),
                Span::styled(
                    evt.client_id.clone(),
                    Style::default().fg(Color::Cyan),
                ),
            ])
        })
        .collect();

    let flow = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Live Data Flow (xrce/sensors) ")
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        );

    frame.render_widget(flow, area);
}

fn draw_actuator_queue(frame: &mut Frame, area: Rect, state: &AppState) {
    let lines: Vec<Line> = state
        .actuator_queue
        .iter()
        .map(|cmd| {
            let age = cmd.timestamp.elapsed();
            let age_str = if age.as_secs() > 0 {
                format!("{}s ago", age.as_secs())
            } else {
                format!("{}ms ago", age.as_millis())
            };

            let value_str = if cmd.value == 1.0 { "ON" } else { "OFF" };

            Line::from(vec![
                Span::styled(
                    format!("[{}] ", age_str),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{} ", cmd.command_type),
                    Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                ),
                Span::raw("-> "),
                Span::styled(
                    cmd.target.clone(),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(format!(" = {}", value_str)),
            ])
        })
        .collect();

    let queue = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Actuator Queue (xrce/actuators) ")
                .title_style(
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
        );

    frame.render_widget(queue, area);
}

/// Format a byte count as human-readable.
fn format_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
