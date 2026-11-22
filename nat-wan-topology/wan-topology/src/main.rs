// CONCEPT DEMO - Simulated data for showcase purposes
//!
//! WAN Topology Viewer
//!
//! An egui desktop application that visualizes a WAN network topology
//! with NAT traversal, ICE candidates, and site-to-site connections.
//!
//! The viewer simulates 3 WAN sites (Paris, Tokyo, NYC) with different
//! NAT types and shows animated connection establishment.

use eframe::egui;
use rand::Rng;
use std::time::Instant;

// -- Data structures ----------------------------------------------------------

/// ICE connection establishment phase
#[derive(Debug, Clone, Copy, PartialEq)]
enum IcePhase {
    Gathering,
    Checking,
    Connected,
    Failed,
}

impl IcePhase {
    fn label(self) -> &'static str {
        match self {
            IcePhase::Gathering => "Gathering",
            IcePhase::Checking => "Checking",
            IcePhase::Connected => "Connected",
            IcePhase::Failed => "Failed",
        }
    }
}

/// Information about a single WAN site
#[derive(Debug, Clone)]
struct SiteInfo {
    name: String,
    public_ip: String,
    public_port: i32,
    nat_type: String,
    using_relay: bool,
    rtt_ms: f64,
    /// Position on screen (fraction of panel size)
    pos_frac: (f32, f32),
    /// ICE candidates for this site
    candidates: Vec<CandidateInfo>,
    /// Heartbeat counter
    heartbeat_count: u64,
    /// Last heartbeat time
    last_heartbeat: Instant,
}

/// ICE candidate information
#[derive(Debug, Clone)]
struct CandidateInfo {
    candidate_id: String,
    candidate_type: String,
    address: String,
    port: i32,
    priority: i32,
}

/// Connection between two sites
#[derive(Debug, Clone)]
struct Connection {
    from: usize,
    to: usize,
    rtt_ms: f64,
    phase: IcePhase,
    /// When this connection started establishing
    started_at: Instant,
    /// Total messages sent on this connection
    message_count: u64,
    /// Total bytes sent
    bytes_sent: u64,
    /// Whether this connection uses a relay
    via_relay: bool,
}

/// Aggregate statistics
#[derive(Debug, Default)]
struct Stats {
    total_messages: u64,
    total_bytes: u64,
    avg_rtt_ms: f64,
    relay_connections: u32,
    direct_connections: u32,
    failed_connections: u32,
}

// -- Application state --------------------------------------------------------

struct TopologyApp {
    sites: Vec<SiteInfo>,
    connections: Vec<Connection>,
    stats: Stats,
    start_time: Instant,
    last_update: Instant,
    /// Animation tick counter
    tick: u64,
    /// Selected site index for detail panel
    selected_site: Option<usize>,
}

impl TopologyApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let now = Instant::now();

        let sites = vec![
            SiteInfo {
                name: "Paris".to_string(),
                public_ip: "192.0.2.101".to_string(),
                public_port: 7400,
                nat_type: "full_cone".to_string(),
                using_relay: false,
                rtt_ms: 15.0,
                pos_frac: (0.15, 0.35),
                candidates: vec![
                    CandidateInfo {
                        candidate_id: "paris-host-0".to_string(),
                        candidate_type: "host".to_string(),
                        address: "192.168.1.100".to_string(),
                        port: 7400,
                        priority: 2130706431,
                    },
                    CandidateInfo {
                        candidate_id: "paris-srflx-0".to_string(),
                        candidate_type: "srflx".to_string(),
                        address: "192.0.2.101".to_string(),
                        port: 8400,
                        priority: 1694498815,
                    },
                    CandidateInfo {
                        candidate_id: "paris-relay-0".to_string(),
                        candidate_type: "relay".to_string(),
                        address: "203.0.113.1".to_string(),
                        port: 3478,
                        priority: 16777215,
                    },
                ],
                heartbeat_count: 0,
                last_heartbeat: now,
            },
            SiteInfo {
                name: "Tokyo".to_string(),
                public_ip: "198.51.100.22".to_string(),
                public_port: 7401,
                nat_type: "symmetric".to_string(),
                using_relay: true,
                rtt_ms: 120.0,
                pos_frac: (0.85, 0.25),
                candidates: vec![
                    CandidateInfo {
                        candidate_id: "tokyo-host-0".to_string(),
                        candidate_type: "host".to_string(),
                        address: "10.0.0.50".to_string(),
                        port: 7401,
                        priority: 2130706431,
                    },
                    CandidateInfo {
                        candidate_id: "tokyo-srflx-0".to_string(),
                        candidate_type: "srflx".to_string(),
                        address: "198.51.100.22".to_string(),
                        port: 8401,
                        priority: 1694498815,
                    },
                    CandidateInfo {
                        candidate_id: "tokyo-relay-0".to_string(),
                        candidate_type: "relay".to_string(),
                        address: "203.0.113.1".to_string(),
                        port: 3479,
                        priority: 16777215,
                    },
                ],
                heartbeat_count: 0,
                last_heartbeat: now,
            },
            SiteInfo {
                name: "NYC".to_string(),
                public_ip: "203.0.113.33".to_string(),
                public_port: 7402,
                nat_type: "restricted".to_string(),
                using_relay: false,
                rtt_ms: 45.0,
                pos_frac: (0.50, 0.75),
                candidates: vec![
                    CandidateInfo {
                        candidate_id: "nyc-host-0".to_string(),
                        candidate_type: "host".to_string(),
                        address: "172.16.0.10".to_string(),
                        port: 7402,
                        priority: 2130706431,
                    },
                    CandidateInfo {
                        candidate_id: "nyc-srflx-0".to_string(),
                        candidate_type: "srflx".to_string(),
                        address: "203.0.113.33".to_string(),
                        port: 8402,
                        priority: 1694498815,
                    },
                    CandidateInfo {
                        candidate_id: "nyc-relay-0".to_string(),
                        candidate_type: "relay".to_string(),
                        address: "203.0.113.1".to_string(),
                        port: 3480,
                        priority: 16777215,
                    },
                ],
                heartbeat_count: 0,
                last_heartbeat: now,
            },
        ];

        // Create connections between all site pairs
        let connections = vec![
            Connection {
                from: 0,
                to: 1,
                rtt_ms: 140.0, // Paris -> Tokyo
                phase: IcePhase::Gathering,
                started_at: now,
                message_count: 0,
                bytes_sent: 0,
                via_relay: true, // Tokyo is symmetric NAT
            },
            Connection {
                from: 0,
                to: 2,
                rtt_ms: 65.0, // Paris -> NYC
                phase: IcePhase::Gathering,
                started_at: now,
                message_count: 0,
                bytes_sent: 0,
                via_relay: false,
            },
            Connection {
                from: 1,
                to: 2,
                rtt_ms: 170.0, // Tokyo -> NYC
                phase: IcePhase::Gathering,
                started_at: now,
                message_count: 0,
                bytes_sent: 0,
                via_relay: true, // Tokyo is symmetric NAT
            },
        ];

        Self {
            sites,
            connections,
            stats: Stats::default(),
            start_time: now,
            last_update: now,
            tick: 0,
            selected_site: None,
        }
    }

    /// Update simulation state
    fn update_simulation(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f64();

        // Only update at ~10 Hz
        if dt < 0.1 {
            return;
        }
        self.last_update = now;
        self.tick += 1;

        let mut rng = rand::thread_rng();
        let elapsed = now.duration_since(self.start_time).as_secs_f64();

        // Update heartbeats (every ~1 second)
        if self.tick % 10 == 0 {
            for site in &mut self.sites {
                site.heartbeat_count += 1;
                site.last_heartbeat = now;
                // Add RTT jitter
                let jitter: f64 = rng.gen_range(-2.0..2.0);
                site.rtt_ms = match site.name.as_str() {
                    "Paris" => 15.0 + jitter,
                    "Tokyo" => 120.0 + jitter,
                    "NYC" => 45.0 + jitter,
                    _ => 50.0 + jitter,
                };
            }
        }

        // Animate ICE connection phases
        for conn in &mut self.connections {
            let age = now.duration_since(conn.started_at).as_secs_f64();

            conn.phase = if age < 2.0 {
                IcePhase::Gathering
            } else if age < 4.0 {
                IcePhase::Checking
            } else {
                IcePhase::Connected
            };

            // Update RTT with jitter when connected
            if conn.phase == IcePhase::Connected {
                let jitter: f64 = rng.gen_range(-3.0..3.0);
                let base_rtt = match (conn.from, conn.to) {
                    (0, 1) | (1, 0) => 140.0,
                    (0, 2) | (2, 0) => 65.0,
                    (1, 2) | (2, 1) => 170.0,
                    _ => 100.0,
                };
                conn.rtt_ms = base_rtt + jitter;

                // Simulate messages at ~0.5 Hz per connection
                if self.tick % 20 == 0 {
                    conn.message_count += 1;
                    conn.bytes_sent += rng.gen_range(64..1400) as u64;
                }
            }
        }

        // Recompute stats
        self.stats = Stats::default();
        for conn in &self.connections {
            self.stats.total_messages += conn.message_count;
            self.stats.total_bytes += conn.bytes_sent;

            match conn.phase {
                IcePhase::Connected if conn.via_relay => self.stats.relay_connections += 1,
                IcePhase::Connected => self.stats.direct_connections += 1,
                IcePhase::Failed => self.stats.failed_connections += 1,
                _ => {}
            }
        }

        let connected: Vec<f64> = self
            .connections
            .iter()
            .filter(|c| c.phase == IcePhase::Connected)
            .map(|c| c.rtt_ms)
            .collect();

        if !connected.is_empty() {
            self.stats.avg_rtt_ms =
                connected.iter().sum::<f64>() / connected.len() as f64;
        }

        // Simulate a connection failure after 15 seconds for dramatic effect,
        // then recover after 25 seconds
        if elapsed > 15.0 && elapsed < 25.0 {
            if let Some(conn) = self.connections.get_mut(2) {
                conn.phase = IcePhase::Failed;
            }
        } else if elapsed > 25.0 {
            if let Some(conn) = self.connections.get_mut(2) {
                if conn.phase == IcePhase::Failed {
                    conn.phase = IcePhase::Connected;
                }
            }
        }
    }

    /// Draw the network topology in the central panel
    fn draw_topology(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        let (response, painter) =
            ui.allocate_painter(available, egui::Sense::click());
        let rect = response.rect;

        // Background
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 22, 30));

        // Draw grid lines for visual reference
        let grid_color = egui::Color32::from_rgba_premultiplied(40, 45, 60, 128);
        let grid_spacing = 60.0;
        let mut x = rect.left();
        while x < rect.right() {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(0.5, grid_color),
            );
            x += grid_spacing;
        }
        let mut y = rect.top();
        while y < rect.bottom() {
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(0.5, grid_color),
            );
            y += grid_spacing;
        }

        // Compute site positions in pixels
        let site_positions: Vec<egui::Pos2> = self
            .sites
            .iter()
            .map(|s| {
                egui::pos2(
                    rect.left() + s.pos_frac.0 * rect.width(),
                    rect.top() + s.pos_frac.1 * rect.height(),
                )
            })
            .collect();

        // Draw connections
        for conn in &self.connections {
            let from_pos = site_positions[conn.from];
            let to_pos = site_positions[conn.to];

            let (color, width) = match conn.phase {
                IcePhase::Connected if conn.via_relay => {
                    (egui::Color32::from_rgb(255, 200, 50), 2.5) // Yellow = relay
                }
                IcePhase::Connected => {
                    (egui::Color32::from_rgb(80, 220, 100), 2.5) // Green = direct
                }
                IcePhase::Checking => {
                    // Animated dashes - use a pulsing effect
                    let pulse = ((self.tick as f32) * 0.3).sin() * 0.5 + 0.5;
                    let alpha = (128.0 + pulse * 127.0) as u8;
                    (egui::Color32::from_rgba_premultiplied(100, 180, 255, alpha), 1.5)
                }
                IcePhase::Gathering => {
                    (egui::Color32::from_rgba_premultiplied(100, 100, 100, 128), 1.0)
                }
                IcePhase::Failed => {
                    (egui::Color32::from_rgb(220, 60, 60), 2.0) // Red = failed
                }
            };

            painter.line_segment(
                [from_pos, to_pos],
                egui::Stroke::new(width, color),
            );

            // RTT label at midpoint
            let mid = egui::pos2(
                (from_pos.x + to_pos.x) / 2.0,
                (from_pos.y + to_pos.y) / 2.0,
            );

            let label = if conn.phase == IcePhase::Connected {
                format!("{:.0}ms", conn.rtt_ms)
            } else {
                conn.phase.label().to_string()
            };

            // Background pill for RTT label
            let label_galley = painter.layout_no_wrap(
                label.clone(),
                egui::FontId::proportional(11.0),
                egui::Color32::WHITE,
            );
            let label_rect = egui::Rect::from_center_size(
                mid,
                label_galley.size() + egui::vec2(8.0, 4.0),
            );
            painter.rect_filled(
                label_rect,
                4.0,
                egui::Color32::from_rgba_premultiplied(30, 30, 40, 220),
            );
            painter.text(
                mid,
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(11.0),
                color,
            );

            // Connection type indicator
            if conn.phase == IcePhase::Connected {
                let type_label = if conn.via_relay { "RELAY" } else { "P2P" };
                let type_pos = egui::pos2(mid.x, mid.y + 14.0);
                painter.text(
                    type_pos,
                    egui::Align2::CENTER_CENTER,
                    type_label,
                    egui::FontId::proportional(9.0),
                    egui::Color32::from_rgb(160, 160, 180),
                );
            }
        }

        // Draw site nodes
        let node_radius = 28.0;
        for (i, site) in self.sites.iter().enumerate() {
            let pos = site_positions[i];

            // Outer glow for selected site
            if self.selected_site == Some(i) {
                painter.circle_filled(
                    pos,
                    node_radius + 6.0,
                    egui::Color32::from_rgba_premultiplied(100, 160, 255, 60),
                );
            }

            // Node circle with NAT-type-based color
            let node_color = match site.nat_type.as_str() {
                "full_cone" => egui::Color32::from_rgb(80, 220, 100),
                "symmetric" => egui::Color32::from_rgb(220, 80, 80),
                "restricted" => egui::Color32::from_rgb(220, 180, 50),
                _ => egui::Color32::from_rgb(150, 150, 150),
            };

            // Pulsing heartbeat ring
            let pulse_phase = (self.tick as f32 * 0.15 + i as f32 * 2.0).sin();
            if pulse_phase > 0.5 {
                let ring_alpha = ((pulse_phase - 0.5) * 2.0 * 80.0) as u8;
                painter.circle_stroke(
                    pos,
                    node_radius + 4.0 + pulse_phase * 6.0,
                    egui::Stroke::new(
                        1.5,
                        egui::Color32::from_rgba_premultiplied(
                            node_color.r(),
                            node_color.g(),
                            node_color.b(),
                            ring_alpha,
                        ),
                    ),
                );
            }

            // Main circle
            painter.circle_filled(pos, node_radius, node_color);
            painter.circle_stroke(
                pos,
                node_radius,
                egui::Stroke::new(2.0, egui::Color32::WHITE),
            );

            // Site name inside circle
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                &site.name,
                egui::FontId::proportional(13.0),
                egui::Color32::WHITE,
            );

            // NAT type badge below
            let badge_pos = egui::pos2(pos.x, pos.y + node_radius + 14.0);
            let badge_text = format!("NAT: {}", site.nat_type);
            let badge_galley = painter.layout_no_wrap(
                badge_text.clone(),
                egui::FontId::proportional(10.0),
                egui::Color32::WHITE,
            );
            let badge_rect = egui::Rect::from_center_size(
                badge_pos,
                badge_galley.size() + egui::vec2(8.0, 4.0),
            );

            let badge_bg = match site.nat_type.as_str() {
                "full_cone" => egui::Color32::from_rgb(30, 80, 40),
                "symmetric" => egui::Color32::from_rgb(80, 30, 30),
                "restricted" => egui::Color32::from_rgb(80, 65, 20),
                _ => egui::Color32::from_rgb(50, 50, 50),
            };
            painter.rect_filled(badge_rect, 3.0, badge_bg);
            painter.text(
                badge_pos,
                egui::Align2::CENTER_CENTER,
                badge_text,
                egui::FontId::proportional(10.0),
                egui::Color32::from_rgb(220, 220, 220),
            );

            // IP:port label
            let ip_pos = egui::pos2(pos.x, pos.y + node_radius + 28.0);
            painter.text(
                ip_pos,
                egui::Align2::CENTER_CENTER,
                format!("{}:{}", site.public_ip, site.public_port),
                egui::FontId::proportional(9.0),
                egui::Color32::from_rgb(140, 140, 160),
            );

            // Check for click to select
            let click_rect =
                egui::Rect::from_center_size(pos, egui::vec2(node_radius * 2.5, node_radius * 2.5));
            if response.clicked() {
                if let Some(click_pos) = response.interact_pointer_pos() {
                    if click_rect.contains(click_pos) {
                        self.selected_site = Some(i);
                    }
                }
            }
        }

        // Title overlay
        painter.text(
            egui::pos2(rect.left() + 12.0, rect.top() + 12.0),
            egui::Align2::LEFT_TOP,
            "HDDS WAN Topology - NAT Traversal Demo",
            egui::FontId::proportional(16.0),
            egui::Color32::from_rgb(200, 200, 220),
        );

        // Elapsed time
        let elapsed = self.start_time.elapsed().as_secs();
        painter.text(
            egui::pos2(rect.right() - 12.0, rect.top() + 12.0),
            egui::Align2::RIGHT_TOP,
            format!("Uptime: {}s", elapsed),
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(160, 160, 180),
        );

        // Legend
        let legend_y = rect.bottom() - 60.0;
        let legend_x = rect.left() + 12.0;
        let legend_items = [
            (egui::Color32::from_rgb(80, 220, 100), "Direct (P2P)"),
            (egui::Color32::from_rgb(255, 200, 50), "Relay (TURN)"),
            (egui::Color32::from_rgb(220, 60, 60), "Failed"),
            (egui::Color32::from_rgb(100, 180, 255), "Establishing"),
        ];
        for (i, (color, label)) in legend_items.iter().enumerate() {
            let lx = legend_x + i as f32 * 120.0;
            painter.circle_filled(egui::pos2(lx, legend_y), 5.0, *color);
            painter.text(
                egui::pos2(lx + 10.0, legend_y),
                egui::Align2::LEFT_CENTER,
                *label,
                egui::FontId::proportional(10.0),
                egui::Color32::from_rgb(180, 180, 200),
            );
        }
    }

    /// Draw the ICE candidates panel
    fn draw_ice_panel(&self, ui: &mut egui::Ui) {
        ui.heading("ICE Candidates");
        ui.separator();

        for site in &self.sites {
            ui.collapsing(format!("{} ({})", site.name, site.candidates.len()), |ui| {
                for candidate in &site.candidates {
                    let type_color = match candidate.candidate_type.as_str() {
                        "host" => egui::Color32::from_rgb(80, 220, 100),
                        "srflx" => egui::Color32::from_rgb(100, 180, 255),
                        "relay" => egui::Color32::from_rgb(255, 200, 50),
                        _ => egui::Color32::GRAY,
                    };
                    ui.horizontal(|ui| {
                        ui.colored_label(type_color, &candidate.candidate_type);
                        ui.label(format!(
                            "{} {}:{} (pri: {})",
                            candidate.candidate_id,
                            candidate.address,
                            candidate.port,
                            candidate.priority,
                        ));
                    });
                }
            });
        }
    }

    /// Draw the stats panel
    fn draw_stats_panel(&self, ui: &mut egui::Ui) {
        ui.heading("Network Stats");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Total messages:");
            ui.strong(format!("{}", self.stats.total_messages));
        });
        ui.horizontal(|ui| {
            ui.label("Total bytes:");
            let kb = self.stats.total_bytes as f64 / 1024.0;
            ui.strong(format!("{:.1} KB", kb));
        });
        ui.horizontal(|ui| {
            ui.label("Avg RTT:");
            ui.strong(format!("{:.1} ms", self.stats.avg_rtt_ms));
        });
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Direct links:");
            ui.colored_label(
                egui::Color32::from_rgb(80, 220, 100),
                format!("{}", self.stats.direct_connections),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Relay links:");
            ui.colored_label(
                egui::Color32::from_rgb(255, 200, 50),
                format!("{}", self.stats.relay_connections),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Failed links:");
            ui.colored_label(
                egui::Color32::from_rgb(220, 60, 60),
                format!("{}", self.stats.failed_connections),
            );
        });

        let total = self.stats.direct_connections
            + self.stats.relay_connections
            + self.stats.failed_connections;
        if total > 0 {
            let relay_pct =
                self.stats.relay_connections as f64 / total as f64 * 100.0;
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Relay usage:");
                ui.strong(format!("{:.0}%", relay_pct));
            });
        }
    }

    /// Draw site detail panel (when a site is selected)
    fn draw_site_detail(&self, ui: &mut egui::Ui) {
        if let Some(idx) = self.selected_site {
            let site = &self.sites[idx];
            ui.heading(format!("Site: {}", site.name));
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Public IP:");
                ui.strong(&site.public_ip);
            });
            ui.horizontal(|ui| {
                ui.label("Port:");
                ui.strong(format!("{}", site.public_port));
            });
            ui.horizontal(|ui| {
                ui.label("NAT type:");
                let color = match site.nat_type.as_str() {
                    "full_cone" => egui::Color32::from_rgb(80, 220, 100),
                    "symmetric" => egui::Color32::from_rgb(220, 80, 80),
                    "restricted" => egui::Color32::from_rgb(220, 180, 50),
                    _ => egui::Color32::GRAY,
                };
                ui.colored_label(color, &site.nat_type);
            });
            ui.horizontal(|ui| {
                ui.label("Using relay:");
                ui.strong(if site.using_relay { "Yes" } else { "No" });
            });
            ui.horizontal(|ui| {
                ui.label("RTT to relay:");
                ui.strong(format!("{:.1} ms", site.rtt_ms));
            });
            ui.horizontal(|ui| {
                ui.label("Heartbeats:");
                ui.strong(format!("{}", site.heartbeat_count));
            });

            ui.separator();
            ui.label("Connections:");
            for conn in &self.connections {
                if conn.from == idx || conn.to == idx {
                    let other = if conn.from == idx { conn.to } else { conn.from };
                    let other_name = &self.sites[other].name;
                    let phase_color = match conn.phase {
                        IcePhase::Connected if conn.via_relay => {
                            egui::Color32::from_rgb(255, 200, 50)
                        }
                        IcePhase::Connected => egui::Color32::from_rgb(80, 220, 100),
                        IcePhase::Failed => egui::Color32::from_rgb(220, 60, 60),
                        _ => egui::Color32::from_rgb(100, 180, 255),
                    };
                    ui.horizontal(|ui| {
                        ui.colored_label(phase_color, conn.phase.label());
                        ui.label(format!(
                            "-> {} ({:.0}ms, {} msgs)",
                            other_name, conn.rtt_ms, conn.message_count
                        ));
                    });
                }
            }
        } else {
            ui.label("Click a site node to see details");
        }
    }
}

impl eframe::App for TopologyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update simulation
        self.update_simulation();

        // Request repaint for animations
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        // Right panel: ICE candidates + stats + site detail
        egui::SidePanel::right("info_panel")
            .min_width(260.0)
            .default_width(300.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_stats_panel(ui);
                    ui.add_space(12.0);
                    self.draw_site_detail(ui);
                    ui.add_space(12.0);
                    self.draw_ice_panel(ui);
                });
            });

        // Central panel: topology visualization
        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_topology(ui);
        });
    }
}

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("wan_topology=info".parse().unwrap()),
        )
        .init();

    tracing::info!("Starting WAN Topology Viewer");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 700.0])
            .with_title("HDDS WAN Topology - NAT Traversal Demo"),
        ..Default::default()
    };

    eframe::run_native(
        "HDDS WAN Topology",
        native_options,
        Box::new(|cc| Ok(Box::new(TopologyApp::new(cc)))),
    )
}
