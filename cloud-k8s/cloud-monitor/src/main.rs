// CONCEPT DEMO - Simulated data for showcase purposes
//!
//! Cloud Monitor - ratatui K8s Cluster Monitor TUI
//!
//! Displays a real-time dashboard for monitoring simulated K8s pods:
//! 1. Pod Grid: Table of pods with CPU%, Mem, Msg/s, status (color-coded)
//! 2. Discovery Events: Scrolling log of join/leave events
//! 3. Scaling Panel: Auto-scaling decisions when CPU thresholds are crossed
//! 4. Cross-Site Stats: RTT and relay status for cross-site messages
//! 5. Header: Cluster summary with pod count and uptime

// Re-export types for documentation; actual DDS readers would use these in production
#[allow(unused_imports)]
use cloud_types::{
    timestamp_ns, topics, CrossSiteMessage, DiscoveryEvent, ScalingDecision, WorkloadMetric,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use hdds::{Participant, QoS, TransportMode};
use rand::Rng;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame, Terminal,
};
use std::collections::HashMap;
use std::io::stdout;
use std::time::{Duration, Instant};

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

/// State for a single pod in the monitor
#[derive(Debug, Clone)]
struct PodState {
    pod_name: String,
    #[allow(dead_code)]
    namespace: String,
    node_name: String,
    cpu_percent: f64,
    memory_mb: f64,
    active_connections: i32,
    messages_per_second: i32,
    last_seen: Instant,
    status: PodStatus,
}

#[derive(Debug, Clone, PartialEq)]
enum PodStatus {
    Running,
    HighLoad,
    Critical,
    Terminating,
}

impl PodStatus {
    fn label(&self) -> &str {
        match self {
            PodStatus::Running => "Running",
            PodStatus::HighLoad => "HighLoad",
            PodStatus::Critical => "Critical",
            PodStatus::Terminating => "Termin.",
        }
    }

    fn color(&self) -> Color {
        match self {
            PodStatus::Running => Color::Green,
            PodStatus::HighLoad => Color::Yellow,
            PodStatus::Critical => Color::Red,
            PodStatus::Terminating => Color::DarkGray,
        }
    }
}

/// Discovery event log entry
#[derive(Debug, Clone)]
struct DiscoveryLogEntry {
    timestamp: Instant,
    message: String,
    is_join: bool,
}

/// Scaling decision log entry
#[derive(Debug, Clone)]
struct ScalingLogEntry {
    timestamp: Instant,
    message: String,
}

/// Cross-site stats
#[derive(Debug, Clone)]
struct CrossSiteStats {
    total_messages: u64,
    avg_rtt_ms: f64,
    relayed_count: u64,
    direct_count: u64,
    last_source: String,
    last_dest: String,
}

impl Default for CrossSiteStats {
    fn default() -> Self {
        Self {
            total_messages: 0,
            avg_rtt_ms: 0.0,
            relayed_count: 0,
            direct_count: 0,
            last_source: String::new(),
            last_dest: String::new(),
        }
    }
}

/// Application state
struct App {
    pods: HashMap<String, PodState>,
    discovery_log: Vec<DiscoveryLogEntry>,
    scaling_log: Vec<ScalingLogEntry>,
    cross_site: CrossSiteStats,
    start_time: Instant,
    last_sim_tick: Instant,
    next_scaling_check: Instant,
    next_cross_site: Instant,
    sim_tick_count: u64,
    next_decision_id: u32,
    running: bool,
}

impl App {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            pods: HashMap::new(),
            discovery_log: Vec::new(),
            scaling_log: Vec::new(),
            cross_site: CrossSiteStats::default(),
            start_time: now,
            last_sim_tick: now,
            next_scaling_check: now + Duration::from_secs(10),
            next_cross_site: now + Duration::from_secs(5),
            sim_tick_count: 0,
            next_decision_id: 1,
            running: true,
        }
    }

    /// Initialize with 3 default pods
    fn init_pods(&mut self) {
        let nodes = ["node-a", "node-b", "node-c"];
        for i in 1..=3 {
            let pod_name = format!("worker-{}", i);
            let node = nodes[(i - 1) % nodes.len()];
            self.add_pod(&pod_name, "hdds-demo", node);
        }
    }

    fn add_pod(&mut self, name: &str, namespace: &str, node: &str) {
        let mut rng = rand::thread_rng();
        let pod = PodState {
            pod_name: name.to_string(),
            namespace: namespace.to_string(),
            node_name: node.to_string(),
            cpu_percent: rng.gen_range(10.0..30.0),
            memory_mb: rng.gen_range(100.0..200.0),
            active_connections: rng.gen_range(1..5),
            messages_per_second: rng.gen_range(50..200),
            last_seen: Instant::now(),
            status: PodStatus::Running,
        };
        self.pods.insert(name.to_string(), pod);
        self.discovery_log.push(DiscoveryLogEntry {
            timestamp: Instant::now(),
            message: format!("[k8s] {} joined on {}", name, node),
            is_join: true,
        });
    }

    fn remove_pod(&mut self, name: &str) {
        if let Some(pod) = self.pods.get_mut(name) {
            pod.status = PodStatus::Terminating;
            self.discovery_log.push(DiscoveryLogEntry {
                timestamp: Instant::now(),
                message: format!("[k8s] {} leaving cluster", name),
                is_join: false,
            });
        }
    }

    /// Simulate metric updates for all pods
    fn simulate_tick(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_sim_tick) < Duration::from_millis(500) {
            return;
        }
        self.last_sim_tick = now;
        self.sim_tick_count += 1;

        let elapsed = now.duration_since(self.start_time).as_secs_f64();
        let mut rng = rand::thread_rng();

        // Update metrics for each running pod
        let pod_names: Vec<String> = self.pods.keys().cloned().collect();
        for name in &pod_names {
            if let Some(pod) = self.pods.get_mut(name) {
                if pod.status == PodStatus::Terminating {
                    continue;
                }

                // CPU: sinusoidal base + noise, with periodic spikes
                let base_sin = ((elapsed * 0.08).sin() * 20.0).abs();
                let noise = rng.gen_range(-5.0..5.0);
                let spike = if (elapsed % 30.0) < 5.0 {
                    rng.gen_range(20.0..40.0)
                } else {
                    0.0
                };
                pod.cpu_percent = (20.0 + base_sin + noise + spike).clamp(5.0, 98.0);

                // Memory: slow growth
                pod.memory_mb += rng.gen_range(-1.0..2.0);
                pod.memory_mb = pod.memory_mb.clamp(100.0, 500.0);

                // Connections and throughput
                pod.active_connections =
                    ((pod.cpu_percent / 10.0) as i32 + rng.gen_range(0..3)).max(1);
                pod.messages_per_second =
                    ((pod.cpu_percent * 8.0) as i32 + rng.gen_range(-30..30)).max(10);

                // Update status based on CPU
                pod.status = if pod.cpu_percent > 80.0 {
                    PodStatus::Critical
                } else if pod.cpu_percent > 50.0 {
                    PodStatus::HighLoad
                } else {
                    PodStatus::Running
                };

                pod.last_seen = now;
            }
        }

        // Remove terminated pods after a delay
        self.pods.retain(|_, p| {
            if p.status == PodStatus::Terminating {
                now.duration_since(p.last_seen) < Duration::from_secs(3)
            } else {
                true
            }
        });

        // Auto-scaling check
        if now >= self.next_scaling_check {
            self.next_scaling_check = now + Duration::from_secs(8);
            self.check_scaling();
        }

        // Cross-site message simulation
        if now >= self.next_cross_site {
            self.next_cross_site = now + Duration::from_secs(rng.gen_range(3..8));
            self.simulate_cross_site();
        }

        // Trim logs
        if self.discovery_log.len() > 50 {
            self.discovery_log.drain(0..self.discovery_log.len() - 50);
        }
        if self.scaling_log.len() > 20 {
            self.scaling_log.drain(0..self.scaling_log.len() - 20);
        }
    }

    fn check_scaling(&mut self) {
        let running_pods: Vec<&PodState> = self
            .pods
            .values()
            .filter(|p| p.status != PodStatus::Terminating)
            .collect();

        let current_count = running_pods.len() as i32;
        if current_count == 0 {
            return;
        }

        let avg_cpu: f64 =
            running_pods.iter().map(|p| p.cpu_percent).sum::<f64>() / running_pods.len() as f64;

        if avg_cpu > 70.0 && current_count < 8 {
            // Scale up
            let desired = (current_count + 1).min(8);
            let new_pod_name = format!("worker-{}", self.pods.len() + 1);
            let nodes = ["node-a", "node-b", "node-c"];
            let node = nodes[self.pods.len() % nodes.len()];

            self.scaling_log.push(ScalingLogEntry {
                timestamp: Instant::now(),
                message: format!(
                    "SCALE UP hdds-worker: {} -> {} (avg CPU {:.0}% > 70%)",
                    current_count, desired, avg_cpu
                ),
            });

            let decision_id = self.next_decision_id;
            self.next_decision_id += 1;

            // Log the decision (would publish via DDS in real scenario)
            tracing::info!(
                "Scaling decision #{}: scale up to {} replicas (avg CPU: {:.1}%)",
                decision_id,
                desired,
                avg_cpu
            );

            self.add_pod(&new_pod_name, "hdds-demo", node);
        } else if avg_cpu < 30.0 && current_count > 3 {
            // Scale down
            let desired = (current_count - 1).max(3);

            self.scaling_log.push(ScalingLogEntry {
                timestamp: Instant::now(),
                message: format!(
                    "SCALE DOWN hdds-worker: {} -> {} (avg CPU {:.0}% < 30%)",
                    current_count, desired, avg_cpu
                ),
            });

            // Remove the last pod
            if let Some(last_pod) = self
                .pods
                .keys()
                .max_by_key(|k| k.len() * 1000 + k.as_bytes().last().copied().unwrap_or(0) as usize)
                .cloned()
            {
                self.remove_pod(&last_pod);
            }
        }
    }

    fn simulate_cross_site(&mut self) {
        let mut rng = rand::thread_rng();
        let sites = ["us-east-1", "eu-west-1", "ap-south-1"];
        let src = sites[rng.gen_range(0..sites.len())];
        let dst = sites[rng.gen_range(0..sites.len())];
        let via_relay = rng.gen_bool(0.3);
        let rtt = if via_relay {
            rng.gen_range(80.0..200.0)
        } else {
            rng.gen_range(10.0..60.0)
        };

        self.cross_site.total_messages += 1;
        let n = self.cross_site.total_messages as f64;
        self.cross_site.avg_rtt_ms =
            self.cross_site.avg_rtt_ms * ((n - 1.0) / n) + rtt / n;
        if via_relay {
            self.cross_site.relayed_count += 1;
        } else {
            self.cross_site.direct_count += 1;
        }
        self.cross_site.last_source = src.to_string();
        self.cross_site.last_dest = dst.to_string();
    }

    fn uptime_string(&self) -> String {
        let secs = self.start_time.elapsed().as_secs();
        let mins = secs / 60;
        let hours = mins / 60;
        if hours > 0 {
            format!("{}h {:02}m {:02}s", hours, mins % 60, secs % 60)
        } else if mins > 0 {
            format!("{}m {:02}s", mins, secs % 60)
        } else {
            format!("{}s", secs)
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    // Create DDS participant (for receiving real DDS data if workers are running)
    let _participant = Participant::builder("cloud_monitor")
        .domain_id(DOMAIN_ID)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let _qos = QoS::default();

    // Note: In a real deployment, we would create readers here:
    //   let metric_reader = participant.create_reader::<WorkloadMetric>(...);
    //   let discovery_reader = participant.create_reader::<DiscoveryEvent>(...);
    // For this demo, we simulate the data internally.

    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    app.init_pods();

    // Main loop
    let tick_rate = Duration::from_millis(100);

    loop {
        if !app.running {
            break;
        }

        // Handle input
        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app.running = false;
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            // Manual scale up
                            let n = app.pods.len() + 1;
                            let name = format!("worker-{}", n);
                            let nodes = ["node-a", "node-b", "node-c"];
                            let node = nodes[n % nodes.len()];
                            app.add_pod(&name, "hdds-demo", node);
                            app.scaling_log.push(ScalingLogEntry {
                                timestamp: Instant::now(),
                                message: format!("MANUAL scale up: added {}", name),
                            });
                        }
                        KeyCode::Char('-') => {
                            // Manual scale down
                            if app.pods.len() > 1 {
                                if let Some(last) = app
                                    .pods
                                    .keys()
                                    .max_by_key(|k| {
                                        k.len() * 1000
                                            + k.as_bytes().last().copied().unwrap_or(0) as usize
                                    })
                                    .cloned()
                                {
                                    app.scaling_log.push(ScalingLogEntry {
                                        timestamp: Instant::now(),
                                        message: format!("MANUAL scale down: removing {}", last),
                                    });
                                    app.remove_pod(&last);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Simulate
        app.simulate_tick();

        // Draw
        terminal.draw(|f| draw_ui(f, &app))?;
    }

    // Restore terminal
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    Ok(())
}

fn draw_ui(f: &mut Frame, app: &App) {
    let size = f.area();

    // Main layout: header + body
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(10)])
        .split(size);

    draw_header(f, app, main_chunks[0]);

    // Body: left (pod grid) + right (panels)
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_chunks[1]);

    draw_pod_grid(f, app, body_chunks[0]);

    // Right panels: discovery + scaling + cross-site
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ])
        .split(body_chunks[1]);

    draw_discovery_log(f, app, right_chunks[0]);
    draw_scaling_panel(f, app, right_chunks[1]);
    draw_cross_site_panel(f, app, right_chunks[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let running_count = app
        .pods
        .values()
        .filter(|p| p.status != PodStatus::Terminating)
        .count();
    let avg_cpu: f64 = if running_count > 0 {
        app.pods
            .values()
            .filter(|p| p.status != PodStatus::Terminating)
            .map(|p| p.cpu_percent)
            .sum::<f64>()
            / running_count as f64
    } else {
        0.0
    };

    let header_text = vec![Line::from(vec![
        Span::styled(
            " HDDS Cloud-Native Cluster Monitor ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  |  "),
        Span::styled(
            format!("Pods: {}", running_count),
            Style::default().fg(Color::Green),
        ),
        Span::raw("  |  "),
        Span::styled(
            format!("Avg CPU: {:.1}%", avg_cpu),
            Style::default().fg(if avg_cpu > 80.0 {
                Color::Red
            } else if avg_cpu > 50.0 {
                Color::Yellow
            } else {
                Color::Green
            }),
        ),
        Span::raw("  |  "),
        Span::styled(
            format!("Uptime: {}", app.uptime_string()),
            Style::default().fg(Color::White),
        ),
        Span::raw("  |  "),
        Span::styled(
            "[q] quit  [+/-] scale",
            Style::default().fg(Color::DarkGray),
        ),
    ])];

    let header = Paragraph::new(header_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" HDDS K8s Discovery Demo "),
    );
    f.render_widget(header, area);
}

fn draw_pod_grid(f: &mut Frame, app: &App, area: Rect) {
    let header_cells = ["Name", "Node", "CPU%", "Mem(MB)", "Conn", "Msg/s", "Status"]
        .iter()
        .map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells).height(1);

    let mut pods: Vec<&PodState> = app.pods.values().collect();
    pods.sort_by(|a, b| a.pod_name.cmp(&b.pod_name));

    let rows: Vec<Row> = pods
        .iter()
        .map(|pod| {
            let cpu_color = if pod.cpu_percent > 80.0 {
                Color::Red
            } else if pod.cpu_percent > 50.0 {
                Color::Yellow
            } else {
                Color::Green
            };

            let cpu_bar = format!(
                "{:5.1}% {}",
                pod.cpu_percent,
                bar_string(pod.cpu_percent, 10)
            );

            let cells = vec![
                Cell::from(pod.pod_name.clone()).style(Style::default().fg(Color::White)),
                Cell::from(pod.node_name.clone()).style(Style::default().fg(Color::Cyan)),
                Cell::from(cpu_bar).style(Style::default().fg(cpu_color)),
                Cell::from(format!("{:.0}", pod.memory_mb))
                    .style(Style::default().fg(Color::White)),
                Cell::from(format!("{}", pod.active_connections))
                    .style(Style::default().fg(Color::White)),
                Cell::from(format!("{}", pod.messages_per_second))
                    .style(Style::default().fg(Color::White)),
                Cell::from(pod.status.label()).style(Style::default().fg(pod.status.color())),
            ];
            Row::new(cells).height(1)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(20),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(9),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Pod Grid ")
            .border_style(Style::default().fg(Color::Blue)),
    );

    f.render_widget(table, area);
}

fn draw_discovery_log(f: &mut Frame, app: &App, area: Rect) {
    let max_lines = (area.height as usize).saturating_sub(2);
    let start = if app.discovery_log.len() > max_lines {
        app.discovery_log.len() - max_lines
    } else {
        0
    };

    let lines: Vec<Line> = app.discovery_log[start..]
        .iter()
        .map(|entry| {
            let elapsed = entry.timestamp.elapsed().as_secs();
            let prefix = format!("[{:3}s] ", elapsed);
            let color = if entry.is_join {
                Color::Green
            } else {
                Color::Red
            };
            Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::DarkGray)),
                Span::styled(&entry.message, Style::default().fg(color)),
            ])
        })
        .collect();

    let log = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Discovery Events ")
            .border_style(Style::default().fg(Color::Magenta)),
    );
    f.render_widget(log, area);
}

fn draw_scaling_panel(f: &mut Frame, app: &App, area: Rect) {
    let max_lines = (area.height as usize).saturating_sub(2);
    let start = if app.scaling_log.len() > max_lines {
        app.scaling_log.len() - max_lines
    } else {
        0
    };

    let lines: Vec<Line> = app.scaling_log[start..]
        .iter()
        .map(|entry| {
            let elapsed = entry.timestamp.elapsed().as_secs();
            let prefix = format!("[{:3}s] ", elapsed);
            let color = if entry.message.contains("UP") {
                Color::Green
            } else {
                Color::Yellow
            };
            Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::DarkGray)),
                Span::styled(&entry.message, Style::default().fg(color)),
            ])
        })
        .collect();

    let panel = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Scaling Decisions ")
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(panel, area);
}

fn draw_cross_site_panel(f: &mut Frame, app: &App, area: Rect) {
    let cs = &app.cross_site;
    let lines = if cs.total_messages > 0 {
        vec![
            Line::from(vec![
                Span::styled("Total: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} messages", cs.total_messages),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Avg RTT: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.1} ms", cs.avg_rtt_ms),
                    Style::default().fg(if cs.avg_rtt_ms > 100.0 {
                        Color::Red
                    } else if cs.avg_rtt_ms > 50.0 {
                        Color::Yellow
                    } else {
                        Color::Green
                    }),
                ),
            ]),
            Line::from(vec![
                Span::styled("Direct: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", cs.direct_count),
                    Style::default().fg(Color::Green),
                ),
                Span::styled("  Relayed: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", cs.relayed_count),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::styled("Last: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} -> {}", cs.last_source, cs.last_dest),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
        ]
    } else {
        vec![Line::from(Span::styled(
            "Waiting for cross-site messages...",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    let panel = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Cross-Site Stats ")
            .border_style(Style::default().fg(Color::Green)),
    );
    f.render_widget(panel, area);
}

/// Generate a simple ASCII bar for a percentage value
fn bar_string(percent: f64, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width.saturating_sub(filled);
    format!("[{}{}]", "#".repeat(filled), ".".repeat(empty))
}
