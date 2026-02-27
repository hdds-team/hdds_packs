// CONCEPT DEMO - Simulated data for showcase purposes
//
// HDDS TSN Automotive Dashboard
//
// A ratatui terminal UI that subscribes to all TSN vehicle topics and displays:
//   1. Priority Tier Waterfall - horizontal bars showing avg latency per tier
//   2. Vehicle State - speed, RPM, fuel, battery, oil temp
//   3. TSN Metrics Table - packets_sent, on_time%, p99_latency, jitter per tier
//   4. Recent Events - scrolling log of brake/steering commands
//
// Press 'q' to quit.

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
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, Borders, Cell, Paragraph, Row, Table, Wrap};

use hdds::{Participant, QoS, TransportMode};
use tsn_types::*;

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

/// Holds the latest vehicle state.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct VehicleSnapshot {
    speed_kmh: f64,
    rpm: f64,
    fuel_percent: f64,
    battery_volt: f64,
    oil_temp_c: f64,
    timestamp_ns: i64,
}

impl Default for VehicleSnapshot {
    fn default() -> Self {
        Self {
            speed_kmh: 0.0,
            rpm: 0.0,
            fuel_percent: 0.0,
            battery_volt: 0.0,
            oil_temp_c: 0.0,
            timestamp_ns: 0,
        }
    }
}

/// Holds TSN metrics for a single priority tier.
#[derive(Debug, Clone)]
struct TierMetrics {
    tier: PriorityTier,
    packets_sent: i64,
    packets_on_time: i64,
    packets_late: i64,
    p99_latency_us: f64,
    jitter_us: f64,
}

impl TierMetrics {
    fn new(tier: PriorityTier) -> Self {
        Self {
            tier,
            packets_sent: 0,
            packets_on_time: 0,
            packets_late: 0,
            p99_latency_us: 0.0,
            jitter_us: 0.0,
        }
    }

    fn on_time_percent(&self) -> f64 {
        if self.packets_sent == 0 {
            100.0
        } else {
            self.packets_on_time as f64 / self.packets_sent as f64 * 100.0
        }
    }
}

/// A recent event displayed in the scrolling log.
#[derive(Debug, Clone)]
struct RecentEvent {
    timestamp: Instant,
    text: String,
    is_emergency: bool,
}

/// The shared state updated by the DDS reader thread and rendered by the TUI.
struct AppState {
    vehicle: VehicleSnapshot,
    tier_metrics: [TierMetrics; 4],
    recent_events: VecDeque<RecentEvent>,
    start_time: Instant,
    total_samples: u64,
}

impl AppState {
    fn new() -> Self {
        Self {
            vehicle: VehicleSnapshot::default(),
            tier_metrics: [
                TierMetrics::new(PriorityTier::CRITICAL),
                TierMetrics::new(PriorityTier::HIGH),
                TierMetrics::new(PriorityTier::MEDIUM),
                TierMetrics::new(PriorityTier::LOW),
            ],
            recent_events: VecDeque::with_capacity(64),
            start_time: Instant::now(),
            total_samples: 0,
        }
    }

    fn push_event(&mut self, text: String, is_emergency: bool) {
        self.recent_events.push_front(RecentEvent {
            timestamp: Instant::now(),
            text,
            is_emergency,
        });
        if self.recent_events.len() > 50 {
            self.recent_events.pop_back();
        }
    }
}

// ---------------------------------------------------------------------------
// DDS reader thread
// ---------------------------------------------------------------------------

/// Background thread: reads all TSN topics and updates AppState.
fn dds_reader_loop(participant: Arc<Participant>, state: Arc<Mutex<AppState>>) {
    // Create readers for all topics
    let brake_reader = match participant.create_reader::<BrakeCommand>("tsn/brake", QoS::reliable())
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to create brake reader: {}", e);
            return;
        }
    };

    let steering_reader =
        match participant.create_reader::<SteeringCommand>("tsn/steering", QoS::reliable()) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to create steering reader: {}", e);
                return;
            }
        };

    let telemetry_reader =
        match participant.create_reader::<VehicleTelemetry>("tsn/telemetry", QoS::reliable()) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to create telemetry reader: {}", e);
                return;
            }
        };

    let metrics_reader =
        match participant.create_reader::<TsnMetrics>("tsn/metrics", QoS::default()) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to create metrics reader: {}", e);
                return;
            }
        };

    loop {
        // Poll brake commands
        match brake_reader.take() {
            Ok(Some(cmd)) => {
                let mut st = state.lock().unwrap();
                st.total_samples += 1;
                let event_text = format!(
                    "BRAKE ecu={} pressure={:.1}bar abs={} emergency={}",
                    cmd.ecu_id,
                    cmd.pressure_bar,
                    cmd.abs_active,
                    cmd.emergency,
                );
                st.push_event(event_text, cmd.emergency);
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("Brake reader error: {}", e),
        }

        // Poll steering commands
        match steering_reader.take() {
            Ok(Some(cmd)) => {
                let mut st = state.lock().unwrap();
                st.total_samples += 1;
                let event_text = format!(
                    "STEER ecu={} angle={:+.1}deg torque={:.1}Nm",
                    cmd.ecu_id, cmd.angle_deg, cmd.torque_nm
                );
                st.push_event(event_text, false);
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("Steering reader error: {}", e),
        }

        // Poll vehicle telemetry
        match telemetry_reader.take() {
            Ok(Some(tel)) => {
                let mut st = state.lock().unwrap();
                st.total_samples += 1;
                st.vehicle = VehicleSnapshot {
                    speed_kmh: tel.speed_kmh,
                    rpm: tel.rpm,
                    fuel_percent: tel.fuel_percent,
                    battery_volt: tel.battery_volt,
                    oil_temp_c: tel.oil_temp_c,
                    timestamp_ns: tel.timestamp_ns,
                };
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("Telemetry reader error: {}", e),
        }

        // Poll TSN metrics
        match metrics_reader.take() {
            Ok(Some(m)) => {
                let mut st = state.lock().unwrap();
                st.total_samples += 1;
                if let Some(tier) = st.tier_metrics.iter_mut().find(|t| t.tier == m.tier) {
                    tier.packets_sent = m.packets_sent;
                    tier.packets_on_time = m.packets_on_time;
                    tier.packets_late = m.packets_late;
                    tier.p99_latency_us = m.p99_latency_us;
                    tier.jitter_us = m.jitter_us;
                }
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("Metrics reader error: {}", e),
        }

        // If no data arrives (demo mode without sim), insert placeholder values
        // so the dashboard is never entirely blank.
        {
            let mut st = state.lock().unwrap();
            if st.total_samples == 0 && st.start_time.elapsed() > Duration::from_secs(2) {
                // Show placeholder data when running standalone
                st.vehicle = VehicleSnapshot {
                    speed_kmh: 85.3,
                    rpm: 3200.0,
                    fuel_percent: 72.4,
                    battery_volt: 13.7,
                    oil_temp_c: 92.1,
                    timestamp_ns: 0,
                };
                st.tier_metrics[0] = TierMetrics {
                    tier: PriorityTier::CRITICAL,
                    packets_sent: 1200,
                    packets_on_time: 1197,
                    packets_late: 3,
                    p99_latency_us: 450.0,
                    jitter_us: 85.0,
                };
                st.tier_metrics[1] = TierMetrics {
                    tier: PriorityTier::HIGH,
                    packets_sent: 240,
                    packets_on_time: 238,
                    packets_late: 2,
                    p99_latency_us: 1800.0,
                    jitter_us: 350.0,
                };
                st.tier_metrics[2] = TierMetrics {
                    tier: PriorityTier::MEDIUM,
                    packets_sent: 60,
                    packets_on_time: 58,
                    packets_late: 2,
                    p99_latency_us: 4500.0,
                    jitter_us: 1200.0,
                };
                st.tier_metrics[3] = TierMetrics {
                    tier: PriorityTier::LOW,
                    packets_sent: 12,
                    packets_on_time: 11,
                    packets_late: 1,
                    p99_latency_us: 12000.0,
                    jitter_us: 2800.0,
                };
                st.push_event(
                    "BRAKE ecu=1 pressure=32.5bar abs=false emergency=false".to_string(),
                    false,
                );
                st.push_event(
                    "STEER ecu=2 angle=+5.2deg torque=6.1Nm".to_string(),
                    false,
                );
            }
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

// ---------------------------------------------------------------------------
// TUI
// ---------------------------------------------------------------------------

fn run_tui(state: Arc<Mutex<AppState>>) -> io::Result<()> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let tick_rate = Duration::from_millis(100); // 10Hz refresh

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

    // Top-level layout: header + body
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(10),   // Body
        ])
        .split(area);

    draw_header(frame, outer[0], state);

    // Body: top row (waterfall + vehicle) | bottom row (metrics table + events)
    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Percentage(55),
        ])
        .split(outer[1]);

    // Top row: waterfall (left) + vehicle state (right)
    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(body[0]);

    draw_waterfall(frame, top_row[0], state);
    draw_vehicle_state(frame, top_row[1], state);

    // Bottom row: metrics table (left) + recent events (right)
    let bottom_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
        ])
        .split(body[1]);

    draw_metrics_table(frame, bottom_row[0], state);
    draw_recent_events(frame, bottom_row[1], state);
}

fn draw_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let uptime = state.start_time.elapsed();
    let uptime_str = format!(
        "{:02}:{:02}:{:02}",
        uptime.as_secs() / 3600,
        (uptime.as_secs() % 3600) / 60,
        uptime.as_secs() % 60,
    );

    let total_packets: i64 = state.tier_metrics.iter().map(|t| t.packets_sent).sum();

    let header_text = format!(
        " HDDS TSN Automotive Dashboard  |  Samples: {}  |  DDS Packets: {}  |  Uptime: {}  |  Press 'q' to quit",
        state.total_samples, total_packets, uptime_str,
    );

    let header = Paragraph::new(header_text)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" TSN Vehicle Network ")
                .title_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
        );
    frame.render_widget(header, area);
}

/// Draw the priority tier waterfall as a horizontal bar chart.
fn draw_waterfall(frame: &mut Frame, area: Rect, state: &AppState) {
    // Build bars for each tier with color coding based on latency health
    let bars: Vec<Bar> = state
        .tier_metrics
        .iter()
        .map(|t| {
            let (deadline_us, label) = match t.tier {
                PriorityTier::CRITICAL => (1000.0, "CRITICAL"),
                PriorityTier::HIGH => (5000.0, "HIGH"),
                PriorityTier::MEDIUM => (50000.0, "MEDIUM"),
                PriorityTier::LOW => (100000.0, "LOW"),
            };

            // Color based on latency vs deadline ratio
            let ratio = if deadline_us > 0.0 {
                t.p99_latency_us / deadline_us
            } else {
                0.0
            };
            let color = if ratio < 0.5 {
                Color::Green
            } else if ratio < 0.8 {
                Color::Yellow
            } else {
                Color::Red
            };

            // Scale the value to fit: show microseconds, cap at a reasonable max
            let bar_value = (t.p99_latency_us as u64).min(20000);

            Bar::default()
                .label(Line::from(format!(
                    "{:<8} {:>7.0}us",
                    label, t.p99_latency_us
                )))
                .value(bar_value)
                .style(Style::default().fg(color))
                .value_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        })
        .collect();

    let bar_chart = BarChart::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Priority Tier Latency (p99, us) ")
                .title_style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .data(BarGroup::default().bars(&bars))
        .bar_width(5)
        .bar_gap(2)
        .direction(Direction::Horizontal);

    frame.render_widget(bar_chart, area);
}

/// Draw the current vehicle state panel.
fn draw_vehicle_state(frame: &mut Frame, area: Rect, state: &AppState) {
    let v = &state.vehicle;

    // Speed bar
    let speed_pct = (v.speed_kmh / 130.0 * 100.0).clamp(0.0, 100.0);
    let speed_color = if v.speed_kmh > 110.0 {
        Color::Red
    } else if v.speed_kmh > 80.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    // RPM bar
    let rpm_pct = (v.rpm / 7000.0 * 100.0).clamp(0.0, 100.0);
    let rpm_color = if v.rpm > 5500.0 {
        Color::Red
    } else if v.rpm > 3500.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    // Fuel gauge
    let fuel_color = if v.fuel_percent < 15.0 {
        Color::Red
    } else if v.fuel_percent < 30.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    // Battery
    let batt_color = if v.battery_volt < 12.5 {
        Color::Red
    } else if v.battery_volt < 13.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    // Oil temp
    let oil_color = if v.oil_temp_c > 110.0 {
        Color::Red
    } else if v.oil_temp_c > 100.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Speed:   ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:>6.1} km/h ", v.speed_kmh),
                Style::default().fg(speed_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                make_bar(speed_pct, 20),
                Style::default().fg(speed_color),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  RPM:     ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:>6.0} rpm  ", v.rpm),
                Style::default().fg(rpm_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                make_bar(rpm_pct, 20),
                Style::default().fg(rpm_color),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Fuel:    ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:>6.1} %    ", v.fuel_percent),
                Style::default().fg(fuel_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                make_bar(v.fuel_percent, 20),
                Style::default().fg(fuel_color),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Battery: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:>6.1} V    ", v.battery_volt),
                Style::default().fg(batt_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Oil:     ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:>6.1} C    ", v.oil_temp_c),
                Style::default().fg(oil_color).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Vehicle State ")
            .title_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
    );

    frame.render_widget(widget, area);
}

/// Draw the TSN metrics table.
fn draw_metrics_table(frame: &mut Frame, area: Rect, state: &AppState) {
    let header_cells = ["Tier", "Packets", "On-Time%", "Late", "p99 (us)", "Jitter (us)"]
        .iter()
        .map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = state
        .tier_metrics
        .iter()
        .map(|t| {
            let tier_color = match t.tier {
                PriorityTier::CRITICAL => Color::Red,
                PriorityTier::HIGH => Color::Yellow,
                PriorityTier::MEDIUM => Color::Cyan,
                PriorityTier::LOW => Color::DarkGray,
            };

            let on_time = t.on_time_percent();
            let on_time_color = if on_time >= 99.0 {
                Color::Green
            } else if on_time >= 95.0 {
                Color::Yellow
            } else {
                Color::Red
            };

            Row::new(vec![
                Cell::from(t.tier.name().to_string()).style(
                    Style::default()
                        .fg(tier_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from(format!("{}", t.packets_sent)),
                Cell::from(format!("{:.1}%", on_time)).style(Style::default().fg(on_time_color)),
                Cell::from(format!("{}", t.packets_late)),
                Cell::from(format!("{:.0}", t.p99_latency_us)),
                Cell::from(format!("{:.0}", t.jitter_us)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths).header(header).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" TSN Metrics ")
            .title_style(
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
    );

    frame.render_widget(table, area);
}

/// Draw the recent events scrolling log.
fn draw_recent_events(frame: &mut Frame, area: Rect, state: &AppState) {
    let lines: Vec<Line> = state
        .recent_events
        .iter()
        .map(|evt| {
            let age = evt.timestamp.elapsed();
            let age_str = if age.as_secs() > 0 {
                format!("{}s", age.as_secs())
            } else {
                format!("{}ms", age.as_millis())
            };

            let style = if evt.is_emergency {
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD)
            } else if evt.text.starts_with("BRAKE") {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            Line::from(vec![
                Span::styled(
                    format!("[{:>5}] ", age_str),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(evt.text.clone(), style),
            ])
        })
        .collect();

    let widget = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Recent Events (brake/steering) ")
                .title_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
        );

    frame.render_widget(widget, area);
}

/// Build an ASCII progress bar like [####------].
fn make_bar(percent: f64, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    format!("[{}{}]", "#".repeat(filled), "-".repeat(empty))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Log to stderr so it does not interfere with the TUI
    tracing_subscriber::fmt()
        .with_env_filter("tsn_dashboard=debug,hdds=warn")
        .with_writer(std::io::stderr)
        .init();

    // Create DDS participant
    // Participant::builder().build() returns Arc<Participant>
    let participant = Participant::builder("tsn-dashboard")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    // Shared state
    let state = Arc::new(Mutex::new(AppState::new()));

    // Spawn DDS reader thread
    let reader_participant = Arc::clone(&participant);
    let reader_state = Arc::clone(&state);
    std::thread::spawn(move || {
        dds_reader_loop(reader_participant, reader_state);
    });

    // Run TUI on main thread
    run_tui(state)?;

    Ok(())
}
