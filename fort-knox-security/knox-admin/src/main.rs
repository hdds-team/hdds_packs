// CONCEPT DEMO - Simulated data for showcase purposes
//! Fort Knox Security - egui Admin Panel
//!
//! Administrative dashboard for the Fort Knox security system.
//! Provides permission editing, audit log viewing, certificate status,
//! dynamic permission rules, and security metrics.

use eframe::egui;
use knox_types::*;
use rand::Rng;
use std::collections::VecDeque;
use std::sync::Arc;

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

// ---------------------------------------------------------------------------
// Zone helpers
// ---------------------------------------------------------------------------

const ALL_ZONES: [Zone; ZONE_COUNT] = [
    Zone::PUBLIC_LOBBY,
    Zone::TELLER_AREA,
    Zone::VAULT_CORRIDOR,
    Zone::VAULT_ROOM,
    Zone::SERVER_ROOM,
];

fn zone_from_index(i: usize) -> Zone {
    ALL_ZONES[i % ZONE_COUNT]
}

// ---------------------------------------------------------------------------
// Permission model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SubjectPermission {
    subject: String,
    zones: Vec<bool>, // one per ZONE_COUNT
}

// ---------------------------------------------------------------------------
// Certificate model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct CertEntry {
    entity: String,
    cert_type: String,
    valid: bool,
    issued: String,
    expires: String,
    serial: String,
}

// ---------------------------------------------------------------------------
// Dynamic rule model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DynamicRule {
    id: u32,
    subject: String,
    zone: Zone,
    grant: bool,
    expression: String,
    active: bool,
}

// ---------------------------------------------------------------------------
// Audit log entry (display)
// ---------------------------------------------------------------------------

#[derive(Clone)]
#[allow(dead_code)]
struct AuditRow {
    seq: i64,
    source: String,
    action: String,
    details: String,
    timestamp_ns: i64,
}

// ---------------------------------------------------------------------------
// Admin app state
// ---------------------------------------------------------------------------

struct AdminApp {
    // DDS context (optional)
    _dds_participant: Option<Arc<hdds::dds::Participant>>,
    audit_reader: Option<hdds::DataReader<AuditLogEntry>>,
    permission_writer: Option<hdds::DataWriter<PermissionChange>>,

    // Permissions tab
    permissions: Vec<SubjectPermission>,

    // Audit log tab
    audit_log: VecDeque<AuditRow>,
    audit_filter: String,

    // Certificate status tab
    certificates: Vec<CertEntry>,

    // Dynamic rules tab
    dynamic_rules: Vec<DynamicRule>,
    next_rule_id: u32,
    new_rule_subject: String,
    new_rule_zone: usize,
    new_rule_grant: bool,
    new_rule_expression: String,

    // Metrics tab
    total_sensor_events: u64,
    alerts_info: u64,
    alerts_warning: u64,
    alerts_critical: u64,
    alerts_breach: u64,
    access_denied_count: u64,
    access_granted_count: u64,

    // Current tab
    active_tab: usize,

    // Simulation tick counter
    tick: u64,
}

impl AdminApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Try to set up DDS
        let (participant, audit_reader, permission_writer) = match Self::try_create_dds() {
            Some((p, ar, pw)) => (Some(p), Some(ar), Some(pw)),
            None => (None, None, None),
        };

        Self {
            _dds_participant: participant,
            audit_reader,
            permission_writer,

            permissions: vec![
                SubjectPermission { subject: "Alice Johnson (EMP-001)".into(), zones: vec![true, false, false, false, false] },
                SubjectPermission { subject: "Bob Williams (EMP-002)".into(), zones: vec![true, true, false, false, false] },
                SubjectPermission { subject: "Carol Martinez (EMP-003)".into(), zones: vec![true, true, true, false, false] },
                SubjectPermission { subject: "David Chen (SEC-001)".into(), zones: vec![true, true, true, true, false] },
                SubjectPermission { subject: "Eva Kowalski (SEC-002)".into(), zones: vec![true, true, true, true, false] },
                SubjectPermission { subject: "Frank OBrien (ADM-001)".into(), zones: vec![true, true, true, true, true] },
                SubjectPermission { subject: "Grace Nakamura (ADM-002)".into(), zones: vec![true, true, true, true, true] },
                SubjectPermission { subject: "Hank Visitor (VIS-001)".into(), zones: vec![true, false, false, false, false] },
            ],

            audit_log: VecDeque::with_capacity(200),
            audit_filter: String::new(),

            certificates: vec![
                CertEntry {
                    entity: "knox-sensors".into(), cert_type: "Identity".into(),
                    valid: true, issued: "2026-01-01".into(), expires: "2027-01-01".into(),
                    serial: "A1:B2:C3:D4:E5:01".into(),
                },
                CertEntry {
                    entity: "knox-monitor".into(), cert_type: "Identity".into(),
                    valid: true, issued: "2026-01-01".into(), expires: "2027-01-01".into(),
                    serial: "A1:B2:C3:D4:E5:02".into(),
                },
                CertEntry {
                    entity: "knox-admin".into(), cert_type: "Identity".into(),
                    valid: true, issued: "2026-01-01".into(), expires: "2027-01-01".into(),
                    serial: "A1:B2:C3:D4:E5:03".into(),
                },
                CertEntry {
                    entity: "DDS Domain CA".into(), cert_type: "Root CA".into(),
                    valid: true, issued: "2025-06-01".into(), expires: "2030-06-01".into(),
                    serial: "00:00:00:00:00:01".into(),
                },
                CertEntry {
                    entity: "Permissions CA".into(), cert_type: "Permissions CA".into(),
                    valid: true, issued: "2025-06-01".into(), expires: "2030-06-01".into(),
                    serial: "00:00:00:00:00:02".into(),
                },
                CertEntry {
                    entity: "Governance Signer".into(), cert_type: "Governance".into(),
                    valid: true, issued: "2026-01-01".into(), expires: "2028-01-01".into(),
                    serial: "FF:EE:DD:CC:BB:01".into(),
                },
            ],

            dynamic_rules: vec![
                DynamicRule {
                    id: 1, subject: "SEC-*".into(), zone: Zone::VAULT_ROOM,
                    grant: true, expression: "time_of_day >= 08:00 AND time_of_day <= 18:00".into(),
                    active: true,
                },
                DynamicRule {
                    id: 2, subject: "EMP-*".into(), zone: Zone::TELLER_AREA,
                    grant: true, expression: "weekday IN (Mon,Tue,Wed,Thu,Fri)".into(),
                    active: true,
                },
                DynamicRule {
                    id: 3, subject: "VIS-*".into(), zone: Zone::PUBLIC_LOBBY,
                    grant: true, expression: "escort_present = true".into(),
                    active: true,
                },
            ],
            next_rule_id: 4,
            new_rule_subject: String::new(),
            new_rule_zone: 0,
            new_rule_grant: true,
            new_rule_expression: String::new(),

            total_sensor_events: 0,
            alerts_info: 0,
            alerts_warning: 0,
            alerts_critical: 0,
            alerts_breach: 0,
            access_denied_count: 0,
            access_granted_count: 0,

            active_tab: 0,
            tick: 0,
        }
    }

    fn try_create_dds() -> Option<(
        Arc<hdds::dds::Participant>,
        hdds::DataReader<AuditLogEntry>,
        hdds::DataWriter<PermissionChange>,
    )> {
        let participant = hdds::Participant::builder("knox_admin")
            .domain_id(DOMAIN_ID)
            .with_transport(hdds::TransportMode::UdpMulticast)
            .build()
            .ok()?;

        let qos = hdds::QoS::default();
        let audit_reader = participant
            .topic::<AuditLogEntry>(topics::AUDIT_LOG).ok()?
            .reader()
            .qos(qos.clone())
            .build()
            .ok()?;
        let permission_writer = participant
            .topic::<PermissionChange>(topics::PERMISSION_CHANGE).ok()?
            .writer()
            .qos(qos)
            .build()
            .ok()?;

        Some((participant, audit_reader, permission_writer))
    }

    fn simulate_tick(&mut self) {
        let mut rng = rand::thread_rng();
        self.tick += 1;

        // Simulate sensor events
        self.total_sensor_events += rng.gen_range(15..25);

        // Simulate alerts by severity
        if rng.gen_bool(0.3) {
            match rng.gen_range(0u32..10) {
                0..=5 => self.alerts_info += 1,
                6..=8 => self.alerts_warning += 1,
                9 => self.alerts_critical += 1,
                _ => self.alerts_breach += 1,
            }
        }

        // Simulate access events
        if rng.gen_bool(0.2) {
            if rng.gen_bool(0.8) {
                self.access_granted_count += 1;
            } else {
                self.access_denied_count += 1;
            }
        }

        // Simulate audit log entries
        if rng.gen_bool(0.25) {
            let actions = ["ACCESS_GRANTED", "ACCESS_DENIED", "ALERT_INFO",
                          "ALERT_WARNING", "SENSOR_READ", "PERMISSION_CHECK",
                          "CERT_VERIFY", "VAULT_STATUS"];
            let sources = ["knox-sensors", "knox-monitor", "knox-admin",
                          "vault-controller", "access-control"];
            let details_pool = [
                "Routine sensor check completed",
                "Badge scan at teller entrance",
                "Temperature reading within normal range",
                "Certificate validation successful",
                "Permission rule evaluated",
                "Vault weight check passed",
                "Motion sensor calibration",
                "Camera feed integrity verified",
            ];

            let seq = (self.audit_log.len() + 1) as i64;
            self.audit_log.push_front(AuditRow {
                seq,
                source: sources[rng.gen_range(0..sources.len())].to_string(),
                action: actions[rng.gen_range(0..actions.len())].to_string(),
                details: details_pool[rng.gen_range(0..details_pool.len())].to_string(),
                timestamp_ns: timestamp_ns(),
            });
            if self.audit_log.len() > 200 {
                self.audit_log.pop_back();
            }
        }
    }

    fn drain_dds(&mut self) {
        if let Some(ref reader) = self.audit_reader {
            while let Ok(Some(entry)) = reader.take() {
                self.audit_log.push_front(AuditRow {
                    seq: entry.sequence_number,
                    source: entry.source_entity,
                    action: entry.action,
                    details: entry.details,
                    timestamp_ns: entry.timestamp_ns,
                });
                if self.audit_log.len() > 200 {
                    self.audit_log.pop_back();
                }
            }
        }
    }

    fn publish_permission_change(&self, subject: &str, zone: Zone, grant: bool) {
        if let Some(ref writer) = self.permission_writer {
            let change = PermissionChange {
                change_id: format!("PC-{}", self.tick),
                subject: subject.to_string(),
                zone,
                grant,
                rule_expression: "admin_override".to_string(),
                effective_ns: timestamp_ns(),
                authorized_by: "knox-admin".to_string(),
            };
            let _ = writer.write(&change);
        }
    }
}

impl eframe::App for AdminApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain DDS data first
        self.drain_dds();

        // Simulate data for display
        self.simulate_tick();

        // Request repaint at ~2Hz
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        // Top panel with tabs
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Fort Knox - Admin Panel");
                ui.separator();
                if ui.selectable_label(self.active_tab == 0, "Permissions").clicked() {
                    self.active_tab = 0;
                }
                if ui.selectable_label(self.active_tab == 1, "Audit Log").clicked() {
                    self.active_tab = 1;
                }
                if ui.selectable_label(self.active_tab == 2, "Certificates").clicked() {
                    self.active_tab = 2;
                }
                if ui.selectable_label(self.active_tab == 3, "Dynamic Rules").clicked() {
                    self.active_tab = 3;
                }
                if ui.selectable_label(self.active_tab == 4, "Metrics").clicked() {
                    self.active_tab = 4;
                }
            });
        });

        // Bottom status bar
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let dds_status = if self._dds_participant.is_some() {
                    "DDS: Connected (domain 42)"
                } else {
                    "DDS: Simulated mode"
                };
                ui.label(dds_status);
                ui.separator();
                ui.label(format!("Events: {} | Alerts: {} | Tick: {}",
                    self.total_sensor_events,
                    self.alerts_info + self.alerts_warning + self.alerts_critical + self.alerts_breach,
                    self.tick,
                ));
            });
        });

        // Central panel
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                0 => self.draw_permissions_tab(ui),
                1 => self.draw_audit_tab(ui),
                2 => self.draw_certificates_tab(ui),
                3 => self.draw_rules_tab(ui),
                4 => self.draw_metrics_tab(ui),
                _ => {}
            }
        });
    }
}

impl AdminApp {
    fn draw_permissions_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Permission Editor");
        ui.label("Grant or revoke zone access for each subject. Changes are published via DDS PermissionChange topic.");
        ui.separator();

        // Collect changes to apply after the loop
        let mut changes: Vec<(usize, usize, bool)> = Vec::new();

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("permissions_grid")
                .striped(true)
                .min_col_width(100.0)
                .show(ui, |ui| {
                    // Header row
                    ui.strong("Subject");
                    for i in 0..ZONE_COUNT {
                        ui.strong(zone_from_index(i).name());
                    }
                    ui.strong("Actions");
                    ui.end_row();

                    // Data rows
                    for (pidx, perm) in self.permissions.iter().enumerate() {
                        ui.label(&perm.subject);

                        for (zidx, &has_access) in perm.zones.iter().enumerate() {
                            if has_access {
                                if ui.button("Revoke").clicked() {
                                    changes.push((pidx, zidx, false));
                                }
                            } else if ui.button("Grant").clicked() {
                                changes.push((pidx, zidx, true));
                            }
                        }

                        // Grant All / Revoke All
                        ui.horizontal(|ui| {
                            if ui.small_button("All").clicked() {
                                for z in 0..ZONE_COUNT {
                                    changes.push((pidx, z, true));
                                }
                            }
                            if ui.small_button("None").clicked() {
                                for z in 0..ZONE_COUNT {
                                    changes.push((pidx, z, false));
                                }
                            }
                        });

                        ui.end_row();
                    }
                });
        });

        // Apply collected changes
        for (pidx, zidx, grant) in changes {
            if pidx < self.permissions.len() && zidx < ZONE_COUNT {
                self.permissions[pidx].zones[zidx] = grant;
                self.publish_permission_change(
                    &self.permissions[pidx].subject.clone(),
                    zone_from_index(zidx),
                    grant,
                );
            }
        }
    }

    fn draw_audit_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Audit Log Viewer");
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut self.audit_filter);
            if ui.button("Clear").clicked() {
                self.audit_filter.clear();
            }
        });
        ui.separator();
        ui.label(format!("{} entries", self.audit_log.len()));

        let filter = self.audit_filter.to_lowercase();

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("audit_grid")
                .striped(true)
                .min_col_width(60.0)
                .show(ui, |ui| {
                    ui.strong("#");
                    ui.strong("Source");
                    ui.strong("Action");
                    ui.strong("Details");
                    ui.end_row();

                    for row in &self.audit_log {
                        // Apply filter
                        if !filter.is_empty() {
                            let haystack = format!(
                                "{} {} {} {}",
                                row.source, row.action, row.details, row.seq
                            ).to_lowercase();
                            if !haystack.contains(&filter) {
                                continue;
                            }
                        }

                        ui.label(format!("{}", row.seq));
                        ui.label(&row.source);

                        // Color-code actions
                        let action_color = if row.action.contains("DENIED") || row.action.contains("BREACH") {
                            egui::Color32::from_rgb(217, 83, 79)
                        } else if row.action.contains("WARNING") || row.action.contains("CRITICAL") {
                            egui::Color32::from_rgb(240, 173, 78)
                        } else {
                            egui::Color32::from_rgb(91, 192, 222)
                        };
                        ui.colored_label(action_color, &row.action);
                        ui.label(&row.details);
                        ui.end_row();
                    }
                });
        });
    }

    fn draw_certificates_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("DDS Security Certificate Status");
        ui.label("X.509 certificates used for DDS Security v1.1 authentication and access control.");
        ui.separator();

        egui::Grid::new("cert_grid")
            .striped(true)
            .min_col_width(120.0)
            .show(ui, |ui| {
                ui.strong("Entity");
                ui.strong("Type");
                ui.strong("Status");
                ui.strong("Issued");
                ui.strong("Expires");
                ui.strong("Serial");
                ui.end_row();

                for cert in &self.certificates {
                    ui.label(&cert.entity);
                    ui.label(&cert.cert_type);

                    let (status_text, color) = if cert.valid {
                        ("Valid", egui::Color32::from_rgb(92, 184, 92))
                    } else {
                        ("EXPIRED", egui::Color32::from_rgb(217, 83, 79))
                    };
                    ui.colored_label(color, status_text);

                    ui.label(&cert.issued);
                    ui.label(&cert.expires);
                    ui.label(&cert.serial);
                    ui.end_row();
                }
            });

        ui.separator();
        ui.heading("Governance Document");
        ui.label("Signed governance document controls topic-level security policies:");
        ui.monospace(
            "  protection_kind: ENCRYPT_WITH_ORIGIN_AUTHENTICATION\n\
             \x20 discovery_protection_kind: SIGN\n\
             \x20 liveliness_protection_kind: SIGN\n\
             \x20 rtps_protection_kind: ENCRYPT"
        );
    }

    fn draw_rules_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Dynamic Permission Rules");
        ui.label("Add or remove permission rules in real-time. Changes are distributed via DDS.");
        ui.separator();

        // Existing rules
        let mut to_remove: Option<usize> = None;
        let mut to_toggle: Option<usize> = None;

        egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
            egui::Grid::new("rules_grid")
                .striped(true)
                .min_col_width(80.0)
                .show(ui, |ui| {
                    ui.strong("ID");
                    ui.strong("Subject");
                    ui.strong("Zone");
                    ui.strong("Grant");
                    ui.strong("Expression");
                    ui.strong("Active");
                    ui.strong("Actions");
                    ui.end_row();

                    for (idx, rule) in self.dynamic_rules.iter().enumerate() {
                        ui.label(format!("{}", rule.id));
                        ui.label(&rule.subject);
                        ui.label(rule.zone.name());
                        ui.label(if rule.grant { "GRANT" } else { "DENY" });
                        ui.label(&rule.expression);

                        let active_color = if rule.active {
                            egui::Color32::from_rgb(92, 184, 92)
                        } else {
                            egui::Color32::from_rgb(150, 150, 150)
                        };
                        ui.colored_label(active_color, if rule.active { "Active" } else { "Inactive" });

                        ui.horizontal(|ui| {
                            if ui.small_button(if rule.active { "Disable" } else { "Enable" }).clicked() {
                                to_toggle = Some(idx);
                            }
                            if ui.small_button("Remove").clicked() {
                                to_remove = Some(idx);
                            }
                        });

                        ui.end_row();
                    }
                });
        });

        // Apply deferred mutations
        if let Some(idx) = to_toggle {
            if idx < self.dynamic_rules.len() {
                self.dynamic_rules[idx].active = !self.dynamic_rules[idx].active;
            }
        }
        if let Some(idx) = to_remove {
            if idx < self.dynamic_rules.len() {
                self.dynamic_rules.remove(idx);
            }
        }

        ui.separator();
        ui.heading("Add New Rule");
        ui.horizontal(|ui| {
            ui.label("Subject:");
            ui.text_edit_singleline(&mut self.new_rule_subject);
        });
        ui.horizontal(|ui| {
            ui.label("Zone:");
            egui::ComboBox::from_id_salt("new_rule_zone")
                .selected_text(zone_from_index(self.new_rule_zone).name())
                .show_ui(ui, |ui| {
                    for i in 0..ZONE_COUNT {
                        ui.selectable_value(&mut self.new_rule_zone, i, zone_from_index(i).name());
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Action:");
            ui.radio_value(&mut self.new_rule_grant, true, "Grant");
            ui.radio_value(&mut self.new_rule_grant, false, "Deny");
        });
        ui.horizontal(|ui| {
            ui.label("Expression:");
            ui.text_edit_singleline(&mut self.new_rule_expression);
        });

        if ui.button("Add Rule").clicked() && !self.new_rule_subject.is_empty() {
            let rule = DynamicRule {
                id: self.next_rule_id,
                subject: self.new_rule_subject.clone(),
                zone: zone_from_index(self.new_rule_zone),
                grant: self.new_rule_grant,
                expression: self.new_rule_expression.clone(),
                active: true,
            };
            self.next_rule_id += 1;

            // Publish via DDS
            self.publish_permission_change(&rule.subject, rule.zone, rule.grant);

            self.dynamic_rules.push(rule);
            self.new_rule_subject.clear();
            self.new_rule_expression.clear();
        }
    }

    fn draw_metrics_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Security Metrics Dashboard");
        ui.separator();

        // Summary cards
        ui.columns(3, |cols| {
            cols[0].group(|ui| {
                ui.heading("Sensor Events");
                ui.label(format!("{}", self.total_sensor_events));
                ui.label("Total readings processed");
            });
            cols[1].group(|ui| {
                ui.heading("Total Alerts");
                let total = self.alerts_info + self.alerts_warning
                    + self.alerts_critical + self.alerts_breach;
                ui.label(format!("{}", total));
                ui.label("Across all severity levels");
            });
            cols[2].group(|ui| {
                ui.heading("Access Events");
                let total = self.access_granted_count + self.access_denied_count;
                ui.label(format!("{}", total));
                let deny_pct = if total > 0 {
                    (self.access_denied_count as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                ui.label(format!("Denied: {:.1}%", deny_pct));
            });
        });

        ui.separator();

        // Alerts by severity
        ui.heading("Alerts by Severity");
        egui::Grid::new("severity_grid")
            .min_col_width(150.0)
            .show(ui, |ui| {
                ui.strong("Severity");
                ui.strong("Count");
                ui.strong("Bar");
                ui.end_row();

                let max_count = *[self.alerts_info, self.alerts_warning,
                    self.alerts_critical, self.alerts_breach]
                    .iter().max().unwrap_or(&1).max(&1);

                let severities = [
                    ("INFO", self.alerts_info, egui::Color32::from_rgb(91, 192, 222)),
                    ("WARNING", self.alerts_warning, egui::Color32::from_rgb(240, 173, 78)),
                    ("CRITICAL", self.alerts_critical, egui::Color32::from_rgb(217, 83, 79)),
                    ("BREACH", self.alerts_breach, egui::Color32::from_rgb(255, 0, 0)),
                ];

                for (name, count, color) in &severities {
                    ui.colored_label(*color, *name);
                    ui.label(format!("{}", count));

                    let bar_width = (*count as f32 / max_count as f32) * 300.0;
                    let (rect, _resp) = ui.allocate_exact_size(
                        egui::vec2(300.0, 18.0),
                        egui::Sense::hover(),
                    );
                    let bar_rect = egui::Rect::from_min_size(
                        rect.min,
                        egui::vec2(bar_width, 18.0),
                    );
                    ui.painter().rect_filled(bar_rect, 2.0, *color);

                    ui.end_row();
                }
            });

        ui.separator();

        // Access control stats
        ui.heading("Access Control");
        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.label(format!("Granted: {}", self.access_granted_count));
                ui.colored_label(
                    egui::Color32::from_rgb(92, 184, 92),
                    "Authorized access events",
                );
            });
            cols[1].group(|ui| {
                ui.label(format!("Denied: {}", self.access_denied_count));
                ui.colored_label(
                    egui::Color32::from_rgb(217, 83, 79),
                    "Unauthorized access attempts",
                );
            });
        });

        ui.separator();

        // DDS Security info
        ui.heading("DDS Security Configuration");
        ui.monospace(
            "Authentication: DDS:Auth:PKI-DH (X.509 + Diffie-Hellman)\n\
             Access Control: DDS:Access:Permissions (signed XML)\n\
             Crypto:         DDS:Crypto:AES-GCM-GMAC (AES-256-GCM)\n\
             Key Exchange:   DDS:Crypto:+Exchange\n\
             Discovery:      SEDP protection: SIGN\n\
             RTPS:           ENCRYPT"
        );
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    tracing::info!("=============================================");
    tracing::info!("  Fort Knox Security - Admin Panel (egui)");
    tracing::info!("  HDDS DDS Security Concept Demo");
    tracing::info!("=============================================");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 750.0])
            .with_title("Fort Knox - Security Admin Panel"),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "Fort Knox Admin",
        options,
        Box::new(|cc| Ok(Box::new(AdminApp::new(cc)))),
    );
}
