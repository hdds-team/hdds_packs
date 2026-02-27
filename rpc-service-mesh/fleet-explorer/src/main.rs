// CONCEPT DEMO - Simulated data for showcase purposes
//
//! Fleet Explorer - GTK4 Service Mesh Browser
//!
//! A graphical tool to discover, inspect, and test RPC services
//! in the fleet management service mesh.
//!
//! Features:
//!   - Service discovery via DDS ServiceRegistration topic
//!   - Real-time health monitoring
//!   - Interactive RPC call testing with JSON payloads
//!   - Status bar with mesh statistics

use fleet_types::*;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText, Label, ListStore,
    Notebook, Orientation, ScrolledWindow, Separator, TextView, TreeView, TreeViewColumn,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const APP_ID: &str = "com.hdds.fleet_explorer";

// ============================================================================
// Simulated service mesh state (standalone demo)
// ============================================================================

#[derive(Debug, Clone)]
struct ServiceInfo {
    name: String,
    service_type: String,
    endpoint: String,
    healthy: bool,
    last_heartbeat_ns: i64,
    methods: Vec<String>,
}

#[derive(Debug, Clone)]
struct MeshState {
    services: HashMap<String, ServiceInfo>,
    participant_count: u32,
    uptime_s: u64,
    rpc_calls: u64,
}

impl MeshState {
    fn new_simulated() -> Self {
        let mut services = HashMap::new();

        services.insert(
            "VehicleService".to_string(),
            ServiceInfo {
                name: "VehicleService".to_string(),
                service_type: "fleet.vehicle.v1".to_string(),
                endpoint: "dds://rpc/VehicleService".to_string(),
                healthy: true,
                last_heartbeat_ns: timestamp_ns(),
                methods: vec![
                    "RegisterVehicle".to_string(),
                    "LocateVehicle".to_string(),
                ],
            },
        );

        services.insert(
            "RouteService".to_string(),
            ServiceInfo {
                name: "RouteService".to_string(),
                service_type: "fleet.route.v1".to_string(),
                endpoint: "dds://rpc/RouteService".to_string(),
                healthy: true,
                last_heartbeat_ns: timestamp_ns(),
                methods: vec!["ComputeRoute".to_string()],
            },
        );

        services.insert(
            "MaintenanceService".to_string(),
            ServiceInfo {
                name: "MaintenanceService".to_string(),
                service_type: "fleet.maintenance.v1".to_string(),
                endpoint: "dds://rpc/MaintenanceService".to_string(),
                healthy: true,
                last_heartbeat_ns: timestamp_ns(),
                methods: vec![
                    "CreateTicket".to_string(),
                    "GetTicketStatus".to_string(),
                ],
            },
        );

        services.insert(
            "AlertService".to_string(),
            ServiceInfo {
                name: "AlertService".to_string(),
                service_type: "fleet.alert.v1".to_string(),
                endpoint: "dds://rpc/AlertService".to_string(),
                healthy: true,
                last_heartbeat_ns: timestamp_ns(),
                methods: vec!["RaiseAlert".to_string()],
            },
        );

        Self {
            services,
            participant_count: 2,
            uptime_s: 0,
            rpc_calls: 0,
        }
    }

    /// Generate a simulated RPC response based on service + method + JSON input.
    fn simulate_rpc_call(&mut self, service: &str, method: &str, input_json: &str) -> String {
        self.rpc_calls += 1;

        match (service, method) {
            ("VehicleService", "RegisterVehicle") => {
                let plate = extract_json_field(input_json, "plate_number")
                    .unwrap_or_else(|| "XX-000-YY".to_string());
                serde_json::to_string_pretty(&serde_json::json!({
                    "vehicle_id": format!("VH-{:03}", 11 + self.rpc_calls),
                    "success": true,
                    "error": "",
                    "_note": format!("Registered plate {}", plate)
                }))
                .unwrap_or_default()
            }
            ("VehicleService", "LocateVehicle") => {
                let vid = extract_json_field(input_json, "vehicle_id")
                    .unwrap_or_else(|| "VH-001".to_string());
                serde_json::to_string_pretty(&serde_json::json!({
                    "vehicle_id": vid,
                    "latitude": 48.8566,
                    "longitude": 2.3522,
                    "state": VehicleState::EN_ROUTE.name(),
                    "speed_kmh": 42.5,
                    "last_update_ns": timestamp_ns()
                }))
                .unwrap_or_default()
            }
            ("RouteService", "ComputeRoute") => {
                serde_json::to_string_pretty(&serde_json::json!({
                    "route_id": format!("RT-{:06}", self.rpc_calls),
                    "distance_km": 12.7,
                    "estimated_minutes": 18.3,
                    "success": true
                }))
                .unwrap_or_default()
            }
            ("MaintenanceService", "CreateTicket") => {
                serde_json::to_string_pretty(&serde_json::json!({
                    "ticket_id": format!("TK-{:06}", self.rpc_calls),
                    "success": true,
                    "error": ""
                }))
                .unwrap_or_default()
            }
            ("MaintenanceService", "GetTicketStatus") => {
                let tid = extract_json_field(input_json, "ticket_id")
                    .unwrap_or_else(|| "TK-000001".to_string());
                serde_json::to_string_pretty(&serde_json::json!({
                    "ticket_id": tid,
                    "vehicle_id": "VH-003",
                    "description": "Brake pad replacement",
                    "priority": 2,
                    "status": TicketStatus::IN_PROGRESS.name(),
                    "created_ns": timestamp_ns() - 3_600_000_000_000i64,
                    "updated_ns": timestamp_ns()
                }))
                .unwrap_or_default()
            }
            ("AlertService", "RaiseAlert") => {
                serde_json::to_string_pretty(&serde_json::json!({
                    "alert_id": format!("AL-{:06}", self.rpc_calls),
                    "acknowledged": false,
                    "error": ""
                }))
                .unwrap_or_default()
            }
            _ => {
                format!("{{ \"error\": \"Unknown method {}/{}\" }}", service, method)
            }
        }
    }
}

/// Extract a simple string field from JSON (basic parser for demo).
fn extract_json_field(json: &str, field: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(json).ok()?;
    parsed
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Generate example JSON payload for a given service method.
fn example_payload(service: &str, method: &str) -> String {
    match (service, method) {
        ("VehicleService", "RegisterVehicle") => {
            serde_json::to_string_pretty(&serde_json::json!({
                "plate_number": "AB-123-CD",
                "model": "Renault Kangoo",
                "max_payload_kg": 800.0,
                "home_lat": 48.8566,
                "home_lon": 2.3522
            }))
            .unwrap_or_default()
        }
        ("VehicleService", "LocateVehicle") => {
            serde_json::to_string_pretty(&serde_json::json!({
                "vehicle_id": "VH-001"
            }))
            .unwrap_or_default()
        }
        ("RouteService", "ComputeRoute") => {
            serde_json::to_string_pretty(&serde_json::json!({
                "origin_lat": 48.8566,
                "origin_lon": 2.3522,
                "dest_lat": 48.8738,
                "dest_lon": 2.2950,
                "avoid_tolls": false
            }))
            .unwrap_or_default()
        }
        ("MaintenanceService", "CreateTicket") => {
            serde_json::to_string_pretty(&serde_json::json!({
                "vehicle_id": "VH-003",
                "description": "Oil change required",
                "priority": 2
            }))
            .unwrap_or_default()
        }
        ("MaintenanceService", "GetTicketStatus") => {
            serde_json::to_string_pretty(&serde_json::json!({
                "ticket_id": "TK-000001"
            }))
            .unwrap_or_default()
        }
        ("AlertService", "RaiseAlert") => {
            serde_json::to_string_pretty(&serde_json::json!({
                "vehicle_id": "VH-005",
                "severity": AlertSeverity::CRITICAL as u32,
                "message": "Engine overheating",
                "latitude": 48.8530,
                "longitude": 2.3499
            }))
            .unwrap_or_default()
        }
        _ => "{}".to_string(),
    }
}

// ============================================================================
// GTK4 Application
// ============================================================================

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let state = Rc::new(RefCell::new(MeshState::new_simulated()));

    // ========================================================================
    // Main window
    // ========================================================================
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Fleet Explorer - HDDS RPC Service Mesh")
        .default_width(1200)
        .default_height(700)
        .build();

    let root_box = GtkBox::new(Orientation::Vertical, 0);

    // ========================================================================
    // Top bar with title
    // ========================================================================
    let title_bar = GtkBox::new(Orientation::Horizontal, 8);
    title_bar.set_margin_start(12);
    title_bar.set_margin_end(12);
    title_bar.set_margin_top(8);
    title_bar.set_margin_bottom(4);

    let title_label = Label::new(Some("HDDS Fleet Service Mesh Explorer"));
    title_label.add_css_class("title-2");
    title_bar.append(&title_label);

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    title_bar.append(&spacer);

    let domain_label = Label::new(Some("Domain: 42 | Transport: UDP Multicast"));
    domain_label.add_css_class("dim-label");
    title_bar.append(&domain_label);

    root_box.append(&title_bar);
    root_box.append(&Separator::new(Orientation::Horizontal));

    // ========================================================================
    // Main content: notebook with tabs
    // ========================================================================
    let notebook = Notebook::new();
    notebook.set_vexpand(true);

    // --- Tab 1: Service Discovery ---
    let discovery_page = build_discovery_tab(&state);
    notebook.append_page(&discovery_page, Some(&Label::new(Some("Service Discovery"))));

    // --- Tab 2: RPC Test Panel ---
    let rpc_page = build_rpc_tab(&state);
    notebook.append_page(&rpc_page, Some(&Label::new(Some("RPC Test"))));

    // --- Tab 3: Mesh Overview ---
    let overview_page = build_overview_tab(&state);
    notebook.append_page(&overview_page, Some(&Label::new(Some("Mesh Overview"))));

    root_box.append(&notebook);

    // ========================================================================
    // Status bar
    // ========================================================================
    root_box.append(&Separator::new(Orientation::Horizontal));

    let status_bar = GtkBox::new(Orientation::Horizontal, 16);
    status_bar.set_margin_start(12);
    status_bar.set_margin_end(12);
    status_bar.set_margin_top(4);
    status_bar.set_margin_bottom(4);

    let participants_label = Label::new(Some("Participants: 2"));
    participants_label.add_css_class("dim-label");
    status_bar.append(&participants_label);

    let services_label = Label::new(Some("Services: 4"));
    services_label.add_css_class("dim-label");
    status_bar.append(&services_label);

    let uptime_label = Label::new(Some("Uptime: 0s"));
    uptime_label.add_css_class("dim-label");
    status_bar.append(&uptime_label);

    let rpc_count_label = Label::new(Some("RPC Calls: 0"));
    rpc_count_label.add_css_class("dim-label");
    status_bar.append(&rpc_count_label);

    root_box.append(&status_bar);

    window.set_child(Some(&root_box));

    // ========================================================================
    // Periodic update timer (1s)
    // ========================================================================
    let state_timer = Rc::clone(&state);
    let uptime_lbl = uptime_label.clone();
    let rpc_lbl = rpc_count_label.clone();
    let svc_lbl = services_label.clone();
    let part_lbl = participants_label.clone();

    glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
        let mut mesh = state_timer.borrow_mut();
        mesh.uptime_s += 1;

        // Toggle health randomly for demo effect
        if mesh.uptime_s % 15 == 0 {
            // Simulate a brief health blip on one service
            if let Some(svc) = mesh.services.get_mut("MaintenanceService") {
                svc.healthy = !svc.healthy;
            }
        } else if mesh.uptime_s % 15 == 2 {
            if let Some(svc) = mesh.services.get_mut("MaintenanceService") {
                svc.healthy = true;
            }
        }

        // Update heartbeat timestamps
        for svc in mesh.services.values_mut() {
            if svc.healthy {
                svc.last_heartbeat_ns = timestamp_ns();
            }
        }

        uptime_lbl.set_text(&format!("Uptime: {}s", mesh.uptime_s));
        rpc_lbl.set_text(&format!("RPC Calls: {}", mesh.rpc_calls));
        svc_lbl.set_text(&format!("Services: {}", mesh.services.len()));
        part_lbl.set_text(&format!("Participants: {}", mesh.participant_count));

        glib::ControlFlow::Continue
    });

    window.present();
}

// ============================================================================
// Tab 1: Service Discovery
// ============================================================================

fn build_discovery_tab(state: &Rc<RefCell<MeshState>>) -> GtkBox {
    let page = GtkBox::new(Orientation::Horizontal, 8);
    page.set_margin_start(8);
    page.set_margin_end(8);
    page.set_margin_top(8);
    page.set_margin_bottom(8);

    // --- Left: Service list (TreeView) ---
    let left_box = GtkBox::new(Orientation::Vertical, 4);
    left_box.set_width_request(400);

    let list_label = Label::new(Some("Discovered Services"));
    list_label.add_css_class("heading");
    list_label.set_halign(gtk4::Align::Start);
    left_box.append(&list_label);

    // ListStore: name, type, status, endpoint
    let store = ListStore::new(&[
        glib::Type::STRING,
        glib::Type::STRING,
        glib::Type::STRING,
        glib::Type::STRING,
    ]);

    // Populate store from state
    {
        let mesh = state.borrow();
        let mut names: Vec<_> = mesh.services.keys().collect();
        names.sort();
        for name in names {
            let svc = &mesh.services[name];
            let status = if svc.healthy { "Healthy" } else { "Degraded" };
            store.set(
                &store.append(),
                &[
                    (0, &svc.name),
                    (1, &svc.service_type),
                    (2, &status),
                    (3, &svc.endpoint),
                ],
            );
        }
    }

    let tree = TreeView::with_model(&store);
    tree.set_headers_visible(true);

    // Column: Name
    let col_name = TreeViewColumn::new();
    col_name.set_title("Service");
    col_name.set_resizable(true);
    col_name.set_min_width(120);
    let cell_name = gtk4::CellRendererText::new();
    col_name.pack_start(&cell_name, true);
    col_name.add_attribute(&cell_name, "text", 0);
    tree.append_column(&col_name);

    // Column: Type
    let col_type = TreeViewColumn::new();
    col_type.set_title("Type");
    col_type.set_resizable(true);
    let cell_type = gtk4::CellRendererText::new();
    col_type.pack_start(&cell_type, true);
    col_type.add_attribute(&cell_type, "text", 1);
    tree.append_column(&col_type);

    // Column: Status
    let col_status = TreeViewColumn::new();
    col_status.set_title("Status");
    col_status.set_resizable(true);
    let cell_status = gtk4::CellRendererText::new();
    col_status.pack_start(&cell_status, true);
    col_status.add_attribute(&cell_status, "text", 2);
    tree.append_column(&col_status);

    // Column: Endpoint
    let col_ep = TreeViewColumn::new();
    col_ep.set_title("Endpoint");
    col_ep.set_resizable(true);
    let cell_ep = gtk4::CellRendererText::new();
    col_ep.pack_start(&cell_ep, true);
    col_ep.add_attribute(&cell_ep, "text", 3);
    tree.append_column(&col_ep);

    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();
    scroll.set_child(Some(&tree));
    left_box.append(&scroll);

    // Refresh button
    let refresh_btn = Button::with_label("Refresh Services");
    let store_ref = store.clone();
    let state_ref = Rc::clone(state);
    refresh_btn.connect_clicked(move |_| {
        store_ref.clear();
        let mesh = state_ref.borrow();
        let mut names: Vec<_> = mesh.services.keys().collect();
        names.sort();
        for name in names {
            let svc = &mesh.services[name];
            let status = if svc.healthy { "Healthy" } else { "Degraded" };
            store_ref.set(
                &store_ref.append(),
                &[
                    (0, &svc.name),
                    (1, &svc.service_type),
                    (2, &status),
                    (3, &svc.endpoint),
                ],
            );
        }
    });
    left_box.append(&refresh_btn);

    page.append(&left_box);
    page.append(&Separator::new(Orientation::Vertical));

    // --- Right: Service detail panel ---
    let detail_box = GtkBox::new(Orientation::Vertical, 8);
    detail_box.set_hexpand(true);
    detail_box.set_margin_start(8);

    let detail_title = Label::new(Some("Service Details"));
    detail_title.add_css_class("heading");
    detail_title.set_halign(gtk4::Align::Start);
    detail_box.append(&detail_title);

    let detail_name = Label::new(Some("Select a service from the list"));
    detail_name.set_halign(gtk4::Align::Start);
    detail_name.set_selectable(true);
    detail_box.append(&detail_name);

    let detail_type = Label::new(Some(""));
    detail_type.set_halign(gtk4::Align::Start);
    detail_type.set_selectable(true);
    detail_box.append(&detail_type);

    let detail_endpoint = Label::new(Some(""));
    detail_endpoint.set_halign(gtk4::Align::Start);
    detail_endpoint.set_selectable(true);
    detail_box.append(&detail_endpoint);

    let detail_health = Label::new(Some(""));
    detail_health.set_halign(gtk4::Align::Start);
    detail_box.append(&detail_health);

    let detail_methods = Label::new(Some(""));
    detail_methods.set_halign(gtk4::Align::Start);
    detail_methods.set_wrap(true);
    detail_box.append(&detail_methods);

    // Connect selection changed
    let selection = tree.selection();
    let state_sel = Rc::clone(state);
    selection.connect_changed(move |sel| {
        if let Some((model, iter)) = sel.selected() {
            let name: String = model.get(&iter, 0);
            let mesh = state_sel.borrow();
            if let Some(svc) = mesh.services.get(&name) {
                detail_name.set_text(&format!("Name: {}", svc.name));
                detail_type.set_text(&format!("Type: {}", svc.service_type));
                detail_endpoint.set_text(&format!("Endpoint: {}", svc.endpoint));
                let health_str = if svc.healthy {
                    "Status: HEALTHY"
                } else {
                    "Status: DEGRADED"
                };
                detail_health.set_text(health_str);
                let methods_str = svc.methods.join(", ");
                detail_methods.set_text(&format!("Methods: {}", methods_str));
            }
        }
    });

    page.append(&detail_box);
    page
}

// ============================================================================
// Tab 2: RPC Test Panel
// ============================================================================

fn build_rpc_tab(state: &Rc<RefCell<MeshState>>) -> GtkBox {
    let page = GtkBox::new(Orientation::Vertical, 8);
    page.set_margin_start(12);
    page.set_margin_end(12);
    page.set_margin_top(12);
    page.set_margin_bottom(12);

    // Service + Method selection row
    let selector_row = GtkBox::new(Orientation::Horizontal, 8);

    let svc_label = Label::new(Some("Service:"));
    selector_row.append(&svc_label);

    let service_combo = ComboBoxText::new();
    service_combo.append_text("VehicleService");
    service_combo.append_text("RouteService");
    service_combo.append_text("MaintenanceService");
    service_combo.append_text("AlertService");
    service_combo.set_active(Some(0));
    selector_row.append(&service_combo);

    let method_label = Label::new(Some("Method:"));
    selector_row.append(&method_label);

    let method_combo = ComboBoxText::new();
    method_combo.append_text("RegisterVehicle");
    method_combo.append_text("LocateVehicle");
    method_combo.set_active(Some(0));
    selector_row.append(&method_combo);

    page.append(&selector_row);

    // Update method combo when service changes
    let method_combo_ref = method_combo.clone();
    service_combo.connect_changed(move |combo| {
        method_combo_ref.remove_all();
        if let Some(svc) = combo.active_text() {
            let methods: Vec<&str> = match svc.as_str() {
                "VehicleService" => vec!["RegisterVehicle", "LocateVehicle"],
                "RouteService" => vec!["ComputeRoute"],
                "MaintenanceService" => vec!["CreateTicket", "GetTicketStatus"],
                "AlertService" => vec!["RaiseAlert"],
                _ => vec![],
            };
            for m in methods {
                method_combo_ref.append_text(m);
            }
            method_combo_ref.set_active(Some(0));
        }
    });

    // JSON input area
    let input_label = Label::new(Some("Request JSON:"));
    input_label.set_halign(gtk4::Align::Start);
    page.append(&input_label);

    let input_view = TextView::new();
    input_view.set_monospace(true);
    input_view.set_wrap_mode(gtk4::WrapMode::Word);
    input_view.buffer().set_text(&example_payload("VehicleService", "RegisterVehicle"));

    let input_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .height_request(150)
        .build();
    input_scroll.set_child(Some(&input_view));
    page.append(&input_scroll);

    // Update example payload when method changes
    let input_buf = input_view.buffer();
    let svc_combo_ref = service_combo.clone();
    let method_combo_ref2 = method_combo.clone();
    method_combo.connect_changed(move |_| {
        let svc = svc_combo_ref
            .active_text()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let method = method_combo_ref2
            .active_text()
            .map(|s| s.to_string())
            .unwrap_or_default();
        input_buf.set_text(&example_payload(&svc, &method));
    });

    // Call button
    let button_row = GtkBox::new(Orientation::Horizontal, 8);
    let call_btn = Button::with_label("Send RPC Call");
    call_btn.add_css_class("suggested-action");
    button_row.append(&call_btn);

    let latency_label = Label::new(Some(""));
    latency_label.add_css_class("dim-label");
    button_row.append(&latency_label);

    page.append(&button_row);

    // Response area
    let output_label = Label::new(Some("Response JSON:"));
    output_label.set_halign(gtk4::Align::Start);
    page.append(&output_label);

    let output_view = TextView::new();
    output_view.set_monospace(true);
    output_view.set_editable(false);
    output_view.set_wrap_mode(gtk4::WrapMode::Word);
    output_view.set_vexpand(true);

    let output_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();
    output_scroll.set_child(Some(&output_view));
    page.append(&output_scroll);

    // Connect call button
    let state_rpc = Rc::clone(state);
    let svc_combo_call = service_combo.clone();
    let method_combo_call = method_combo.clone();
    let input_view_call = input_view.clone();
    let output_buf = output_view.buffer();
    call_btn.connect_clicked(move |_| {
        let svc = svc_combo_call
            .active_text()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let method = method_combo_call
            .active_text()
            .map(|s| s.to_string())
            .unwrap_or_default();

        let buf = input_view_call.buffer();
        let start = buf.start_iter();
        let end = buf.end_iter();
        let input_json = buf.text(&start, &end, false).to_string();

        let t0 = std::time::Instant::now();
        let response = state_rpc
            .borrow_mut()
            .simulate_rpc_call(&svc, &method, &input_json);
        let elapsed = t0.elapsed();

        let header = format!(
            "// {} -> {}.{}()\n// Latency: {:.2}ms\n// Timestamp: {}\n\n",
            "fleet-explorer",
            svc,
            method,
            elapsed.as_secs_f64() * 1000.0,
            timestamp_ns(),
        );

        output_buf.set_text(&format!("{}{}", header, response));
        latency_label.set_text(&format!("RTT: {:.2}ms", elapsed.as_secs_f64() * 1000.0));
    });

    page
}

// ============================================================================
// Tab 3: Mesh Overview
// ============================================================================

fn build_overview_tab(_state: &Rc<RefCell<MeshState>>) -> GtkBox {
    let page = GtkBox::new(Orientation::Vertical, 12);
    page.set_margin_start(16);
    page.set_margin_end(16);
    page.set_margin_top(16);
    page.set_margin_bottom(16);

    let title = Label::new(Some("Service Mesh Topology"));
    title.add_css_class("title-3");
    title.set_halign(gtk4::Align::Start);
    page.append(&title);

    // ASCII art topology
    let topology_text = r#"
    +-------------------+       +------------------+
    |  VehicleService   | <---> |  RouteService    |
    |  fleet.vehicle.v1 |       |  fleet.route.v1  |
    |  2 methods        |       |  1 method        |
    +-------------------+       +------------------+
            |                           |
            |    DDS Service Mesh       |
            |    Domain 42              |
            |    UDP Multicast          |
            |                           |
    +-------------------+       +------------------+
    | MaintenanceService| <---> |  AlertService    |
    | fleet.maint.v1    |       |  fleet.alert.v1  |
    |  2 methods        |       |  1 method        |
    +-------------------+       +------------------+

    Topics:
      rpc/ServiceRegistry              (heartbeat)
      rpc/{Service}/{Method}/Request   (typed request)
      rpc/{Service}/{Method}/Reply     (typed reply)
"#;

    let topology_view = TextView::new();
    topology_view.set_monospace(true);
    topology_view.set_editable(false);
    topology_view.buffer().set_text(topology_text);

    let topo_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .height_request(300)
        .build();
    topo_scroll.set_child(Some(&topology_view));
    page.append(&topo_scroll);

    // Stats grid
    page.append(&Separator::new(Orientation::Horizontal));

    let stats_title = Label::new(Some("Topic Statistics"));
    stats_title.add_css_class("title-3");
    stats_title.set_halign(gtk4::Align::Start);
    page.append(&stats_title);

    let stats_store = ListStore::new(&[
        glib::Type::STRING,
        glib::Type::STRING,
        glib::Type::STRING,
    ]);

    let topics_data = [
        (topics::SERVICE_REGISTRY, "ServiceRegistration", "4 writers"),
        (topics::REGISTER_VEHICLE_REQ, "RegisterVehicleRequest", "on-demand"),
        (topics::REGISTER_VEHICLE_REP, "RegisterVehicleReply", "on-demand"),
        (topics::LOCATE_VEHICLE_REQ, "LocateVehicleRequest", "on-demand"),
        (topics::LOCATE_VEHICLE_REP, "LocateVehicleReply", "on-demand"),
        (topics::COMPUTE_ROUTE_REQ, "ComputeRouteRequest", "on-demand"),
        (topics::COMPUTE_ROUTE_REP, "ComputeRouteReply", "on-demand"),
        (topics::CREATE_TICKET_REQ, "CreateTicketRequest", "on-demand"),
        (topics::CREATE_TICKET_REP, "CreateTicketReply", "on-demand"),
        (topics::GET_TICKET_STATUS_REQ, "GetTicketStatusRequest", "on-demand"),
        (topics::GET_TICKET_STATUS_REP, "GetTicketStatusReply", "on-demand"),
        (topics::RAISE_ALERT_REQ, "RaiseAlertRequest", "on-demand"),
        (topics::RAISE_ALERT_REP, "RaiseAlertReply", "on-demand"),
    ];

    for (topic, type_name, rate) in &topics_data {
        stats_store.set(
            &stats_store.append(),
            &[(0, topic), (1, type_name), (2, rate)],
        );
    }

    let stats_tree = TreeView::with_model(&stats_store);
    stats_tree.set_headers_visible(true);

    let col_topic = TreeViewColumn::new();
    col_topic.set_title("Topic");
    col_topic.set_resizable(true);
    col_topic.set_min_width(300);
    let cell_topic = gtk4::CellRendererText::new();
    col_topic.pack_start(&cell_topic, true);
    col_topic.add_attribute(&cell_topic, "text", 0);
    stats_tree.append_column(&col_topic);

    let col_type = TreeViewColumn::new();
    col_type.set_title("DDS Type");
    col_type.set_resizable(true);
    let cell_type = gtk4::CellRendererText::new();
    col_type.pack_start(&cell_type, true);
    col_type.add_attribute(&cell_type, "text", 1);
    stats_tree.append_column(&col_type);

    let col_rate = TreeViewColumn::new();
    col_rate.set_title("Activity");
    col_rate.set_resizable(true);
    let cell_rate = gtk4::CellRendererText::new();
    col_rate.pack_start(&cell_rate, true);
    col_rate.add_attribute(&cell_rate, "text", 2);
    stats_tree.append_column(&col_rate);

    let stats_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();
    stats_scroll.set_child(Some(&stats_tree));
    page.append(&stats_scroll);

    page
}
