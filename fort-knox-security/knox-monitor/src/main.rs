// CONCEPT DEMO - Simulated data for showcase purposes
//! Fort Knox Security - GTK4 Vault Monitor
//!
//! Real-time vault monitoring dashboard using GTK4.
//! Displays zone statuses, sensor readings, alerts, and access events.
//! Creates a DDS participant and readers, but also maintains internal
//! simulated state so the UI works even without the sensor sim running.

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Frame, Grid, Label, Orientation,
    PolicyType, ScrolledWindow,
};
use knox_types::*;
use rand::Rng;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Application state shared with UI callbacks
// ---------------------------------------------------------------------------

struct ZoneState {
    name: &'static str,
    status: Severity,
    last_reading_value: f64,
}

struct AlertEntry {
    alert_id: String,
    zone: Zone,
    severity: Severity,
    description: String,
}

struct AccessEntry {
    person_name: String,
    from_zone: Zone,
    to_zone: Zone,
    granted: bool,
}

struct MonitorState {
    zones: Vec<ZoneState>,
    alerts: VecDeque<AlertEntry>,
    access_log: VecDeque<AccessEntry>,
    vault_door_sealed: bool,
    vault_temp: f64,
    vault_humidity: f64,
    vault_weight: f64,
    vault_expected_weight: f64,
    total_readings: u64,
    total_alerts: u64,
    total_access_events: u64,
}

impl MonitorState {
    fn new() -> Self {
        Self {
            zones: vec![
                ZoneState { name: "Public Lobby", status: Severity::INFO, last_reading_value: 0.7 },
                ZoneState { name: "Teller Area", status: Severity::INFO, last_reading_value: 0.9 },
                ZoneState { name: "Vault Corridor", status: Severity::INFO, last_reading_value: 0.95 },
                ZoneState { name: "Vault Room", status: Severity::INFO, last_reading_value: 0.99 },
                ZoneState { name: "Server Room", status: Severity::INFO, last_reading_value: 0.95 },
            ],
            alerts: VecDeque::with_capacity(100),
            access_log: VecDeque::with_capacity(50),
            vault_door_sealed: true,
            vault_temp: 18.0,
            vault_humidity: 35.0,
            vault_weight: 5000.0,
            vault_expected_weight: 5000.0,
            total_readings: 0,
            total_alerts: 0,
            total_access_events: 0,
        }
    }

    /// Simulate one tick of state updates (used when DDS data is not arriving).
    fn simulate_tick(&mut self) {
        let mut rng = rand::thread_rng();
        self.total_readings += 20; // 20 sensors per tick

        // Randomly update zone statuses
        for zone in &mut self.zones {
            zone.last_reading_value += rng.gen_range(-0.05..0.05);
            zone.last_reading_value = zone.last_reading_value.clamp(0.0, 1.0);

            // Occasionally change zone status
            if rng.gen_bool(0.05) {
                zone.status = match rng.gen_range(0u32..20) {
                    0..=14 => Severity::INFO,
                    15..=17 => Severity::WARNING,
                    18 => Severity::CRITICAL,
                    _ => Severity::BREACH,
                };
            }
        }

        // Generate occasional alert
        if rng.gen_bool(0.15) {
            let zone_idx = rng.gen_range(0..ZONE_COUNT);
            let severity = match rng.gen_range(0u32..10) {
                0..=5 => Severity::INFO,
                6..=8 => Severity::WARNING,
                9 => Severity::CRITICAL,
                _ => Severity::BREACH,
            };
            let zone = zone_from_index(zone_idx);
            let descriptions = [
                "Motion detected in restricted area",
                "Camera feed disruption",
                "Temperature anomaly detected",
                "Door sensor triggered",
                "Vibration threshold exceeded",
                "Weight discrepancy in vault",
                "Unscheduled access attempt",
                "Sensor calibration drift",
            ];
            self.total_alerts += 1;
            let alert = AlertEntry {
                alert_id: format!("ALT-{:06}", self.total_alerts),
                zone,
                severity,
                description: descriptions[rng.gen_range(0..descriptions.len())].to_string(),
            };
            self.alerts.push_front(alert);
            if self.alerts.len() > 100 {
                self.alerts.pop_back();
            }

            // Update zone status to match worst active alert
            if (severity as u32) > (self.zones[zone_idx].status as u32) {
                self.zones[zone_idx].status = severity;
            }
        }

        // Generate occasional access event
        if rng.gen_bool(0.1) {
            let names = ["Alice Johnson", "Bob Williams", "Carol Martinez",
                         "David Chen", "Eva Kowalski", "Frank OBrien"];
            let from = zone_from_index(rng.gen_range(0..ZONE_COUNT));
            let to = zone_from_index(rng.gen_range(0..ZONE_COUNT));
            let granted = rng.gen_bool(0.8);
            self.total_access_events += 1;

            let entry = AccessEntry {
                person_name: names[rng.gen_range(0..names.len())].to_string(),
                from_zone: from,
                to_zone: to,
                granted,
            };
            self.access_log.push_front(entry);
            if self.access_log.len() > 50 {
                self.access_log.pop_back();
            }
        }

        // Update vault state
        self.vault_temp += rng.gen_range(-0.1..0.1);
        self.vault_humidity += rng.gen_range(-0.5..0.5);
        self.vault_weight += rng.gen_range(-0.2..0.2);
        if rng.gen_bool(0.01) {
            self.vault_door_sealed = !self.vault_door_sealed;
        }
    }
}

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
// Try to set up DDS readers (best-effort, non-blocking)
// ---------------------------------------------------------------------------

struct DdsContext {
    _participant: Arc<hdds::dds::Participant>,
    sensor_reader: hdds::DataReader<SensorReading>,
    alert_reader: hdds::DataReader<SecurityAlert>,
    access_reader: hdds::DataReader<AccessEvent>,
    vault_reader: hdds::DataReader<VaultStatus>,
}

fn try_create_dds() -> Option<DdsContext> {
    let participant = match hdds::Participant::builder("knox_monitor")
        .domain_id(42)
        .with_transport(hdds::TransportMode::UdpMulticast)
        .build()
    {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("DDS init failed (running in sim-only mode): {}", e);
            return None;
        }
    };

    let qos = hdds::QoS::default();

    let sensor_reader = participant
        .create_reader::<SensorReading>(topics::SENSOR_READING, qos.clone())
        .ok()?;
    let alert_reader = participant
        .create_reader::<SecurityAlert>(topics::SECURITY_ALERT, qos.clone())
        .ok()?;
    let access_reader = participant
        .create_reader::<AccessEvent>(topics::ACCESS_EVENT, qos.clone())
        .ok()?;
    let vault_reader = participant
        .create_reader::<VaultStatus>(topics::VAULT_STATUS, qos)
        .ok()?;

    tracing::info!("DDS readers created successfully on domain 42");

    Some(DdsContext {
        _participant: participant,
        sensor_reader,
        alert_reader,
        access_reader,
        vault_reader,
    })
}

// ---------------------------------------------------------------------------
// Build the GTK4 UI
// ---------------------------------------------------------------------------

fn build_ui(app: &Application) {
    // Shared state
    let state = Rc::new(RefCell::new(MonitorState::new()));

    // Try to set up DDS (non-fatal if it fails)
    let dds = try_create_dds();
    let dds = Rc::new(RefCell::new(dds));

    // Main window
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Fort Knox - Vault Monitor")
        .default_width(1200)
        .default_height(800)
        .build();

    // Root layout: vertical box
    let root_box = GtkBox::new(Orientation::Vertical, 4);
    root_box.set_margin_top(8);
    root_box.set_margin_bottom(8);
    root_box.set_margin_start(8);
    root_box.set_margin_end(8);

    // -----------------------------------------------------------------------
    // Title bar
    // -----------------------------------------------------------------------
    let title_label = Label::new(Some("FORT KNOX SECURITY MONITORING SYSTEM"));
    title_label.add_css_class("title-1");
    root_box.append(&title_label);

    let subtitle_label = Label::new(Some("DDS Security v1.1 - Real-time Vault Monitor"));
    subtitle_label.add_css_class("subtitle");
    root_box.append(&subtitle_label);

    // -----------------------------------------------------------------------
    // Zone status bar (5 zones side by side)
    // -----------------------------------------------------------------------
    let zone_frame = Frame::new(Some("Zone Status"));
    let zone_box = GtkBox::new(Orientation::Horizontal, 8);
    zone_box.set_margin_top(4);
    zone_box.set_margin_bottom(4);
    zone_box.set_margin_start(4);
    zone_box.set_margin_end(4);
    zone_box.set_homogeneous(true);

    let zone_labels: Vec<Label> = (0..ZONE_COUNT).map(|i| {
        let label = Label::new(Some(zone_from_index(i).name()));
        label.set_width_request(180);
        label.set_height_request(50);
        label.add_css_class("zone-clear");
        zone_box.append(&label);
        label
    }).collect();

    zone_frame.set_child(Some(&zone_box));
    root_box.append(&zone_frame);

    // -----------------------------------------------------------------------
    // Main content: center (sensors grid + vault) | right (access log)
    // -----------------------------------------------------------------------
    let content_box = GtkBox::new(Orientation::Horizontal, 8);
    content_box.set_vexpand(true);

    // Left side: sensor grid + vault status
    let left_box = GtkBox::new(Orientation::Vertical, 8);
    left_box.set_hexpand(true);

    // Sensor readings grid
    let sensor_frame = Frame::new(Some("Sensor Readings by Zone"));
    let sensor_grid = Grid::new();
    sensor_grid.set_row_spacing(4);
    sensor_grid.set_column_spacing(12);
    sensor_grid.set_margin_top(4);
    sensor_grid.set_margin_bottom(4);
    sensor_grid.set_margin_start(4);
    sensor_grid.set_margin_end(4);

    // Column headers
    let headers = ["Zone", "Sensors", "Last Value", "Confidence", "Status"];
    for (col, header) in headers.iter().enumerate() {
        let label = Label::new(Some(header));
        label.add_css_class("heading");
        #[allow(clippy::cast_possible_wrap)]
        sensor_grid.attach(&label, col as i32, 0, 1, 1);
    }

    // Sensor data rows (one per zone)
    let sensor_value_labels: Vec<Vec<Label>> = (0..ZONE_COUNT).map(|row| {
        let zone_label = Label::new(Some(zone_from_index(row).name()));
        zone_label.set_halign(gtk4::Align::Start);
        #[allow(clippy::cast_possible_wrap)]
        sensor_grid.attach(&zone_label, 0, (row + 1) as i32, 1, 1);

        let count_label = Label::new(Some("4"));
        #[allow(clippy::cast_possible_wrap)]
        sensor_grid.attach(&count_label, 1, (row + 1) as i32, 1, 1);

        let value_label = Label::new(Some("--"));
        #[allow(clippy::cast_possible_wrap)]
        sensor_grid.attach(&value_label, 2, (row + 1) as i32, 1, 1);

        let conf_label = Label::new(Some("--"));
        #[allow(clippy::cast_possible_wrap)]
        sensor_grid.attach(&conf_label, 3, (row + 1) as i32, 1, 1);

        let status_label = Label::new(Some("OK"));
        #[allow(clippy::cast_possible_wrap)]
        sensor_grid.attach(&status_label, 4, (row + 1) as i32, 1, 1);

        vec![zone_label, count_label, value_label, conf_label, status_label]
    }).collect();

    sensor_frame.set_child(Some(&sensor_grid));
    left_box.append(&sensor_frame);

    // Vault status panel
    let vault_frame = Frame::new(Some("Vault Status"));
    let vault_grid = Grid::new();
    vault_grid.set_row_spacing(4);
    vault_grid.set_column_spacing(16);
    vault_grid.set_margin_top(4);
    vault_grid.set_margin_bottom(4);
    vault_grid.set_margin_start(8);
    vault_grid.set_margin_end(8);

    let vault_labels_text = [
        "Door:", "Temperature:", "Humidity:", "Weight:", "Expected:", "Anomaly:"
    ];
    let vault_value_labels: Vec<Label> = vault_labels_text.iter().enumerate().map(|(i, text)| {
        let key = Label::new(Some(text));
        key.set_halign(gtk4::Align::Start);
        key.add_css_class("heading");
        #[allow(clippy::cast_possible_wrap)]
        vault_grid.attach(&key, 0, i as i32, 1, 1);

        let val = Label::new(Some("--"));
        val.set_halign(gtk4::Align::Start);
        #[allow(clippy::cast_possible_wrap)]
        vault_grid.attach(&val, 1, i as i32, 1, 1);
        val
    }).collect();

    vault_frame.set_child(Some(&vault_grid));
    left_box.append(&vault_frame);

    // Stats bar
    let stats_frame = Frame::new(Some("Statistics"));
    let stats_box = GtkBox::new(Orientation::Horizontal, 16);
    stats_box.set_margin_top(4);
    stats_box.set_margin_bottom(4);
    stats_box.set_margin_start(8);
    stats_box.set_margin_end(8);
    stats_box.set_homogeneous(true);

    let stats_readings_label = Label::new(Some("Readings: 0"));
    let stats_alerts_label = Label::new(Some("Alerts: 0"));
    let stats_access_label = Label::new(Some("Access Events: 0"));
    stats_box.append(&stats_readings_label);
    stats_box.append(&stats_alerts_label);
    stats_box.append(&stats_access_label);
    stats_frame.set_child(Some(&stats_box));
    left_box.append(&stats_frame);

    content_box.append(&left_box);

    // Right side: access event history
    let access_frame = Frame::new(Some("Access Events"));
    access_frame.set_width_request(350);
    let access_scroll = ScrolledWindow::new();
    access_scroll.set_policy(PolicyType::Never, PolicyType::Automatic);
    access_scroll.set_vexpand(true);

    let access_list_box = GtkBox::new(Orientation::Vertical, 2);
    access_list_box.set_margin_top(4);
    access_list_box.set_margin_bottom(4);
    access_list_box.set_margin_start(4);
    access_list_box.set_margin_end(4);

    let access_placeholder = Label::new(Some("Waiting for access events..."));
    access_list_box.append(&access_placeholder);

    access_scroll.set_child(Some(&access_list_box));
    access_frame.set_child(Some(&access_scroll));
    content_box.append(&access_frame);

    root_box.append(&content_box);

    // -----------------------------------------------------------------------
    // Bottom: scrolling alert log
    // -----------------------------------------------------------------------
    let alert_frame = Frame::new(Some("Security Alerts"));
    alert_frame.set_height_request(200);
    let alert_scroll = ScrolledWindow::new();
    alert_scroll.set_policy(PolicyType::Automatic, PolicyType::Automatic);
    alert_scroll.set_vexpand(false);

    let alert_list_box = GtkBox::new(Orientation::Vertical, 2);
    alert_list_box.set_margin_top(4);
    alert_list_box.set_margin_bottom(4);
    alert_list_box.set_margin_start(4);
    alert_list_box.set_margin_end(4);

    let alert_placeholder = Label::new(Some("No alerts yet..."));
    alert_list_box.append(&alert_placeholder);

    alert_scroll.set_child(Some(&alert_list_box));
    alert_frame.set_child(Some(&alert_scroll));
    root_box.append(&alert_frame);

    window.set_child(Some(&root_box));

    // -----------------------------------------------------------------------
    // CSS styling
    // -----------------------------------------------------------------------
    let css_provider = gtk4::CssProvider::new();
    css_provider.load_from_data(
        "
        .zone-clear { background-color: #2d5a27; color: white; font-weight: bold; padding: 8px; }
        .zone-warning { background-color: #8a6d3b; color: white; font-weight: bold; padding: 8px; }
        .zone-critical { background-color: #a94442; color: white; font-weight: bold; padding: 8px; }
        .zone-breach { background-color: #ff0000; color: white; font-weight: bold; padding: 8px; }
        .alert-info { color: #5bc0de; }
        .alert-warning { color: #f0ad4e; }
        .alert-critical { color: #d9534f; font-weight: bold; }
        .alert-breach { color: #ff0000; font-weight: bold; }
        .access-granted { color: #5cb85c; }
        .access-denied { color: #d9534f; font-weight: bold; }
        .title-1 { font-size: 18px; font-weight: bold; margin-bottom: 4px; }
        .subtitle { font-size: 12px; color: #888888; margin-bottom: 8px; }
        .heading { font-weight: bold; }
        .vault-sealed { color: #5cb85c; font-weight: bold; }
        .vault-open { color: #d9534f; font-weight: bold; }
        ",
    );
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("Could not get default display"),
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // -----------------------------------------------------------------------
    // Periodic UI update (every 500ms)
    // -----------------------------------------------------------------------
    let zone_labels = Rc::new(zone_labels);
    let sensor_value_labels = Rc::new(sensor_value_labels);
    let vault_value_labels = Rc::new(vault_value_labels);
    let alert_list_box = Rc::new(alert_list_box);
    let access_list_box = Rc::new(access_list_box);
    let stats_readings_label = Rc::new(stats_readings_label);
    let stats_alerts_label = Rc::new(stats_alerts_label);
    let stats_access_label = Rc::new(stats_access_label);

    // Track previous alert/access counts to avoid full rebuild
    let prev_alert_count: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
    let prev_access_count: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));

    glib::timeout_add_local(std::time::Duration::from_millis(500), {
        let state = Rc::clone(&state);
        let dds = Rc::clone(&dds);
        let zone_labels = Rc::clone(&zone_labels);
        let sensor_value_labels = Rc::clone(&sensor_value_labels);
        let vault_value_labels = Rc::clone(&vault_value_labels);
        let alert_list_box = Rc::clone(&alert_list_box);
        let access_list_box = Rc::clone(&access_list_box);
        let stats_readings_label = Rc::clone(&stats_readings_label);
        let stats_alerts_label = Rc::clone(&stats_alerts_label);
        let stats_access_label = Rc::clone(&stats_access_label);
        let prev_alert_count = Rc::clone(&prev_alert_count);
        let prev_access_count = Rc::clone(&prev_access_count);

        move || {
            let mut st = state.borrow_mut();

            // Try to read from DDS first
            let mut got_dds_data = false;
            if let Some(ref dds_ctx) = *dds.borrow() {
                // Drain sensor readings
                while let Ok(Some(reading)) = dds_ctx.sensor_reader.take() {
                    got_dds_data = true;
                    st.total_readings += 1;
                    let idx = reading.zone as u32 as usize;
                    if idx < ZONE_COUNT {
                        let z = &mut st.zones[idx];
                        z.last_reading_value = reading.value;
                    }
                }

                // Drain alerts
                while let Ok(Some(alert)) = dds_ctx.alert_reader.take() {
                    got_dds_data = true;
                    st.total_alerts += 1;
                    let zone_idx = alert.zone as u32 as usize;
                    if zone_idx < ZONE_COUNT
                        && (alert.severity as u32) > (st.zones[zone_idx].status as u32)
                    {
                        st.zones[zone_idx].status = alert.severity;
                    }
                    st.alerts.push_front(AlertEntry {
                        alert_id: alert.alert_id,
                        zone: alert.zone,
                        severity: alert.severity,
                        description: alert.description,
                    });
                    if st.alerts.len() > 100 {
                        st.alerts.pop_back();
                    }
                }

                // Drain access events
                while let Ok(Some(event)) = dds_ctx.access_reader.take() {
                    got_dds_data = true;
                    st.total_access_events += 1;
                    st.access_log.push_front(AccessEntry {
                        person_name: event.person_name,
                        from_zone: event.from_zone,
                        to_zone: event.to_zone,
                        granted: event.granted,
                    });
                    if st.access_log.len() > 50 {
                        st.access_log.pop_back();
                    }
                }

                // Drain vault status
                while let Ok(Some(vault)) = dds_ctx.vault_reader.take() {
                    got_dds_data = true;
                    st.vault_door_sealed = vault.door_sealed;
                    st.vault_temp = vault.internal_temperature;
                    st.vault_humidity = vault.internal_humidity;
                    st.vault_weight = vault.weight_kg;
                    st.vault_expected_weight = vault.expected_weight_kg;
                }
            }

            // Fall back to simulation if no DDS data
            if !got_dds_data {
                st.simulate_tick();
            }

            // Update zone status labels
            for (i, label) in zone_labels.iter().enumerate() {
                let zone = &st.zones[i];
                let (css_class, status_text) = match zone.status {
                    Severity::INFO => ("zone-clear", "CLEAR"),
                    Severity::WARNING => ("zone-warning", "WARNING"),
                    Severity::CRITICAL => ("zone-critical", "CRITICAL"),
                    Severity::BREACH => ("zone-breach", "BREACH"),
                };

                // Remove old classes, add new
                label.remove_css_class("zone-clear");
                label.remove_css_class("zone-warning");
                label.remove_css_class("zone-critical");
                label.remove_css_class("zone-breach");
                label.add_css_class(css_class);
                label.set_text(&format!("{}\n[{}]", zone.name, status_text));
            }

            // Decay zone statuses occasionally
            for zone in &mut st.zones {
                if rand::thread_rng().gen_bool(0.05) && (zone.status as u32) > (Severity::INFO as u32) {
                    zone.status = match zone.status {
                        Severity::BREACH => Severity::CRITICAL,
                        Severity::CRITICAL => Severity::WARNING,
                        Severity::WARNING => Severity::INFO,
                        Severity::INFO => Severity::INFO,
                    };
                }
            }

            // Update sensor grid
            for (i, row_labels) in sensor_value_labels.iter().enumerate() {
                let zone = &st.zones[i];
                row_labels[2].set_text(&format!("{:.3}", zone.last_reading_value));
                let conf = 0.85 + rand::thread_rng().gen_range(0.0..0.15_f64);
                row_labels[3].set_text(&format!("{:.1}%", conf * 100.0));
                row_labels[4].set_text(match zone.status {
                    Severity::INFO => "OK",
                    Severity::WARNING => "WARN",
                    Severity::CRITICAL => "CRIT",
                    Severity::BREACH => "BREACH",
                });
            }

            // Update vault status
            vault_value_labels[0].set_text(if st.vault_door_sealed { "SEALED" } else { "OPEN" });
            vault_value_labels[0].remove_css_class("vault-sealed");
            vault_value_labels[0].remove_css_class("vault-open");
            vault_value_labels[0].add_css_class(if st.vault_door_sealed { "vault-sealed" } else { "vault-open" });
            vault_value_labels[1].set_text(&format!("{:.1} C", st.vault_temp));
            vault_value_labels[2].set_text(&format!("{:.1}%", st.vault_humidity));
            vault_value_labels[3].set_text(&format!("{:.1} kg", st.vault_weight));
            vault_value_labels[4].set_text(&format!("{:.1} kg", st.vault_expected_weight));
            let weight_diff = (st.vault_weight - st.vault_expected_weight).abs();
            vault_value_labels[5].set_text(if weight_diff > 2.0 { "YES - INVESTIGATE" } else { "None" });

            // Update stats
            stats_readings_label.set_text(&format!("Readings: {}", st.total_readings));
            stats_alerts_label.set_text(&format!("Alerts: {}", st.total_alerts));
            stats_access_label.set_text(&format!("Access Events: {}", st.total_access_events));

            // Rebuild alert list if count changed
            let current_alert_count = st.alerts.len();
            if current_alert_count != *prev_alert_count.borrow() {
                // Clear existing children
                while let Some(child) = alert_list_box.first_child() {
                    alert_list_box.remove(&child);
                }

                // Show most recent 20 alerts
                for alert in st.alerts.iter().take(20) {
                    let severity_str = alert.severity.name();
                    let zone_str = alert.zone.name();
                    let text = format!(
                        "[{}] {} | {} | {}",
                        alert.alert_id, severity_str, zone_str, alert.description
                    );
                    let label = Label::new(Some(&text));
                    label.set_halign(gtk4::Align::Start);
                    label.set_wrap(true);

                    let css_class = match alert.severity {
                        Severity::INFO => "alert-info",
                        Severity::WARNING => "alert-warning",
                        Severity::CRITICAL => "alert-critical",
                        Severity::BREACH => "alert-breach",
                    };
                    label.add_css_class(css_class);
                    alert_list_box.append(&label);
                }

                *prev_alert_count.borrow_mut() = current_alert_count;
            }

            // Rebuild access list if count changed
            let current_access_count = st.access_log.len();
            if current_access_count != *prev_access_count.borrow() {
                while let Some(child) = access_list_box.first_child() {
                    access_list_box.remove(&child);
                }

                for entry in st.access_log.iter().take(15) {
                    let status = if entry.granted { "GRANTED" } else { "DENIED" };
                    let text = format!(
                        "{} {} | {} -> {}",
                        status,
                        entry.person_name,
                        entry.from_zone.name(),
                        entry.to_zone.name(),
                    );
                    let label = Label::new(Some(&text));
                    label.set_halign(gtk4::Align::Start);
                    label.set_wrap(true);
                    label.add_css_class(if entry.granted { "access-granted" } else { "access-denied" });
                    access_list_box.append(&label);
                }

                *prev_access_count.borrow_mut() = current_access_count;
            }

            glib::ControlFlow::Continue
        }
    });

    window.present();
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    tracing::info!("=============================================");
    tracing::info!("  Fort Knox Security - Vault Monitor (GTK4)");
    tracing::info!("  HDDS DDS Security Concept Demo");
    tracing::info!("=============================================");

    let app = Application::builder()
        .application_id("com.hdds.demo.fort-knox-monitor")
        .build();

    app.connect_activate(build_ui);
    app.run();
}
