// CONCEPT DEMO - Simulated data for showcase purposes
//
//! HDDS QoS Hot-Reload Cockpit Panel
//!
//! FLTK desktop GUI that:
//! - Reads live sensor data from the simulator via DDS
//! - Displays current QoS configuration (mutable vs immutable)
//! - Provides sliders to adjust mutable QoS policies
//! - Writes updated QoS profiles to YAML for hot-reload
//! - Shows a log of all QoS changes with timestamps

use anyhow::Result;
use cockpit_types::*;
use fltk::{
    app,
    button::Button,
    enums::{Align, Color, Font, FrameType},
    frame::Frame,
    group::{Flex, Pack},
    menu::Choice,
    prelude::*,
    text::{TextBuffer, TextDisplay},
    valuator::HorNiceSlider,
    window::Window,
};
use hdds::{Participant, QoS, TransportMode};
use log::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Default QoS profile path
fn qos_profile_path() -> PathBuf {
    let relative = PathBuf::from("qos-profiles/default.yaml");
    if relative.exists() {
        return relative;
    }
    PathBuf::from("/projects/public/demos/hot-reload-cockpit/qos-profiles/default.yaml")
}

/// Format current time as HH:MM:SS
fn time_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (now / 3600) % 24;
    let m = (now / 60) % 60;
    let s = now % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

/// State shared between callbacks
struct PanelState {
    deadline_ms: f64,
    latency_budget_ms: f64,
    transport_priority: f64,
    active_profile: String,
    log_buffer: TextBuffer,
    // Sensor display labels
    sensor_labels: Vec<Frame>,
    // Metrics display labels
    latency_before_label: Frame,
    latency_after_label: Frame,
    throughput_before_label: Frame,
    throughput_after_label: Frame,
    missed_before_label: Frame,
    missed_after_label: Frame,
}

impl PanelState {
    fn append_log(&mut self, msg: &str) {
        let entry = format!("{} {}\n", time_stamp(), msg);
        self.log_buffer.append(&entry);
    }

    fn write_qos_yaml(&mut self) -> std::io::Result<()> {
        let path = qos_profile_path();
        let yaml = format!(
            "profiles:\n  {}:\n    deadline_ms: {}\n    latency_budget_ms: {}\n    transport_priority: {}\n    history_depth: 10\n    reliability: reliable\n",
            self.active_profile,
            self.deadline_ms as u64,
            self.latency_budget_ms as u64,
            self.transport_priority as i32,
        );
        std::fs::write(&path, &yaml)?;
        self.append_log(&format!(
            "Wrote QoS profile '{}' to {}",
            self.active_profile,
            path.display()
        ));
        Ok(())
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("==========================================");
    info!("  HDDS QoS Hot-Reload Cockpit - Panel");
    info!("==========================================");

    // -- Create DDS participant and reader --
    let participant = Participant::builder("cockpit-panel")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::default();
    let sensor_reader =
        participant.topic::<SensorFeed>(topics::SENSOR_FEED)?
            .reader()
            .qos(qos.clone())
            .build()?;
    let snapshot_reader =
        participant.topic::<QosSnapshot>(topics::QOS_SNAPSHOT)?
            .reader()
            .qos(qos.clone())
            .build()?;
    let metrics_reader =
        participant.topic::<MetricsDelta>(topics::METRICS_DELTA)?
            .reader()
            .qos(qos)
            .build()?;

    info!("DDS readers created");

    // -- Build FLTK UI --
    let app = app::App::default().with_scheme(app::Scheme::Gtk);
    app::set_font_size(13);

    let mut wind = Window::default()
        .with_size(900, 700)
        .with_label("HDDS QoS Hot-Reload Cockpit");
    wind.set_color(Color::from_rgb(40, 44, 52));

    // Main vertical layout
    let mut main_pack = Pack::default()
        .with_size(880, 680)
        .with_pos(10, 10);
    main_pack.set_spacing(8);

    // ========== Title ==========
    let mut title = Frame::default().with_size(880, 30);
    title.set_label("HDDS QoS Hot-Reload Cockpit");
    title.set_label_font(Font::HelveticaBold);
    title.set_label_size(18);
    title.set_label_color(Color::from_rgb(97, 175, 239));

    // ========== Profile Selector ==========
    let mut profile_row = Flex::default().with_size(880, 30);
    profile_row.set_type(fltk::group::FlexType::Row);

    let mut profile_label = Frame::default().with_size(120, 30);
    profile_label.set_label("QoS Profile:");
    profile_label.set_label_color(Color::from_rgb(200, 200, 200));
    profile_label.set_align(Align::Left | Align::Inside);

    let mut profile_choice = Choice::default().with_size(200, 30);
    profile_choice.add_choice("default|combat|eco");
    profile_choice.set_value(0);
    profile_choice.set_color(Color::from_rgb(60, 64, 72));
    profile_choice.set_text_color(Color::from_rgb(200, 200, 200));

    let _spacer = Frame::default().with_size(560, 30);

    profile_row.fixed(&profile_label, 120);
    profile_row.fixed(&profile_choice, 200);
    profile_row.end();

    // ========== Sensor Values ==========
    let mut sensor_header = Frame::default().with_size(880, 25);
    sensor_header.set_label("Live Sensor Data");
    sensor_header.set_label_font(Font::HelveticaBold);
    sensor_header.set_label_size(14);
    sensor_header.set_label_color(Color::from_rgb(152, 195, 121));
    sensor_header.set_align(Align::Left | Align::Inside);

    let mut sensor_labels = Vec::new();
    for name in &["Airspeed", "Altitude", "Heading", "Temperature"] {
        let mut lbl = Frame::default().with_size(880, 22);
        lbl.set_label(&format!("  {} : --", name));
        lbl.set_label_color(Color::from_rgb(180, 180, 180));
        lbl.set_label_font(Font::Courier);
        lbl.set_align(Align::Left | Align::Inside);
        sensor_labels.push(lbl);
    }

    // ========== QoS Sliders ==========
    let mut slider_header = Frame::default().with_size(880, 25);
    slider_header.set_label("Mutable QoS Policies (hot-reloadable)");
    slider_header.set_label_font(Font::HelveticaBold);
    slider_header.set_label_size(14);
    slider_header.set_label_color(Color::from_rgb(229, 192, 123));
    slider_header.set_align(Align::Left | Align::Inside);

    // Deadline slider
    let mut deadline_row = Flex::default().with_size(880, 28);
    deadline_row.set_type(fltk::group::FlexType::Row);
    let mut deadline_label = Frame::default().with_size(200, 28);
    deadline_label.set_label("  Deadline (ms): 100");
    deadline_label.set_label_color(Color::from_rgb(180, 180, 180));
    deadline_label.set_align(Align::Left | Align::Inside);
    let mut deadline_slider = HorNiceSlider::default().with_size(680, 28);
    deadline_slider.set_range(1.0, 1000.0);
    deadline_slider.set_value(100.0);
    deadline_slider.set_step(1.0, 1);
    deadline_slider.set_color(Color::from_rgb(60, 64, 72));
    deadline_slider.set_selection_color(Color::from_rgb(97, 175, 239));
    deadline_row.fixed(&deadline_label, 200);
    deadline_row.end();

    // Latency budget slider
    let mut latency_row = Flex::default().with_size(880, 28);
    latency_row.set_type(fltk::group::FlexType::Row);
    let mut latency_label = Frame::default().with_size(200, 28);
    latency_label.set_label("  Latency Budget (ms): 10");
    latency_label.set_label_color(Color::from_rgb(180, 180, 180));
    latency_label.set_align(Align::Left | Align::Inside);
    let mut latency_slider = HorNiceSlider::default().with_size(680, 28);
    latency_slider.set_range(0.0, 100.0);
    latency_slider.set_value(10.0);
    latency_slider.set_step(1.0, 1);
    latency_slider.set_color(Color::from_rgb(60, 64, 72));
    latency_slider.set_selection_color(Color::from_rgb(97, 175, 239));
    latency_row.fixed(&latency_label, 200);
    latency_row.end();

    // Transport priority slider
    let mut priority_row = Flex::default().with_size(880, 28);
    priority_row.set_type(fltk::group::FlexType::Row);
    let mut priority_label = Frame::default().with_size(200, 28);
    priority_label.set_label("  Transport Priority: 3");
    priority_label.set_label_color(Color::from_rgb(180, 180, 180));
    priority_label.set_align(Align::Left | Align::Inside);
    let mut priority_slider = HorNiceSlider::default().with_size(680, 28);
    priority_slider.set_range(0.0, 10.0);
    priority_slider.set_value(3.0);
    priority_slider.set_step(1.0, 1);
    priority_slider.set_color(Color::from_rgb(60, 64, 72));
    priority_slider.set_selection_color(Color::from_rgb(97, 175, 239));
    priority_row.fixed(&priority_label, 200);
    priority_row.end();

    // ========== Apply Button ==========
    let mut apply_btn = Button::default().with_size(880, 35).with_label("Apply QoS Changes");
    apply_btn.set_color(Color::from_rgb(97, 175, 239));
    apply_btn.set_label_color(Color::from_rgb(40, 44, 52));
    apply_btn.set_label_font(Font::HelveticaBold);
    apply_btn.set_label_size(14);

    // ========== Policy Mutability Reference ==========
    let mut policy_header = Frame::default().with_size(880, 25);
    policy_header.set_label("DDS QoS Policy Mutability (per DDS spec)");
    policy_header.set_label_font(Font::HelveticaBold);
    policy_header.set_label_size(14);
    policy_header.set_label_color(Color::from_rgb(198, 120, 221));
    policy_header.set_align(Align::Left | Align::Inside);

    let mut mutable_lbl = Frame::default().with_size(880, 20);
    mutable_lbl.set_label("  MUTABLE:   Deadline, LatencyBudget, TransportPriority, Lifespan, Partition, TimeBasedFilter");
    mutable_lbl.set_label_color(Color::from_rgb(152, 195, 121));
    mutable_lbl.set_label_font(Font::Courier);
    mutable_lbl.set_label_size(11);
    mutable_lbl.set_align(Align::Left | Align::Inside);

    let mut immutable_lbl = Frame::default().with_size(880, 20);
    immutable_lbl.set_label("  IMMUTABLE: Reliability, Durability, History, Ownership, DestinationOrder, ResourceLimits");
    immutable_lbl.set_label_color(Color::from_rgb(224, 108, 117));
    immutable_lbl.set_label_font(Font::Courier);
    immutable_lbl.set_label_size(11);
    immutable_lbl.set_align(Align::Left | Align::Inside);

    // ========== Metrics Before/After ==========
    let mut metrics_header = Frame::default().with_size(880, 25);
    metrics_header.set_label("Before/After Metrics (last QoS change)");
    metrics_header.set_label_font(Font::HelveticaBold);
    metrics_header.set_label_size(14);
    metrics_header.set_label_color(Color::from_rgb(86, 182, 194));
    metrics_header.set_align(Align::Left | Align::Inside);

    let mut latency_before_label = Frame::default().with_size(880, 18);
    latency_before_label.set_label("  Latency BEFORE: -- us");
    latency_before_label.set_label_color(Color::from_rgb(180, 180, 180));
    latency_before_label.set_label_font(Font::Courier);
    latency_before_label.set_label_size(12);
    latency_before_label.set_align(Align::Left | Align::Inside);

    let mut latency_after_label = Frame::default().with_size(880, 18);
    latency_after_label.set_label("  Latency AFTER:  -- us");
    latency_after_label.set_label_color(Color::from_rgb(180, 180, 180));
    latency_after_label.set_label_font(Font::Courier);
    latency_after_label.set_label_size(12);
    latency_after_label.set_align(Align::Left | Align::Inside);

    let mut throughput_before_label = Frame::default().with_size(880, 18);
    throughput_before_label.set_label("  Throughput BEFORE: -- msg/s");
    throughput_before_label.set_label_color(Color::from_rgb(180, 180, 180));
    throughput_before_label.set_label_font(Font::Courier);
    throughput_before_label.set_label_size(12);
    throughput_before_label.set_align(Align::Left | Align::Inside);

    let mut throughput_after_label = Frame::default().with_size(880, 18);
    throughput_after_label.set_label("  Throughput AFTER:  -- msg/s");
    throughput_after_label.set_label_color(Color::from_rgb(180, 180, 180));
    throughput_after_label.set_label_font(Font::Courier);
    throughput_after_label.set_label_size(12);
    throughput_after_label.set_align(Align::Left | Align::Inside);

    let mut missed_before_label = Frame::default().with_size(880, 18);
    missed_before_label.set_label("  Missed Deadlines BEFORE: --");
    missed_before_label.set_label_color(Color::from_rgb(180, 180, 180));
    missed_before_label.set_label_font(Font::Courier);
    missed_before_label.set_label_size(12);
    missed_before_label.set_align(Align::Left | Align::Inside);

    let mut missed_after_label = Frame::default().with_size(880, 18);
    missed_after_label.set_label("  Missed Deadlines AFTER:  --");
    missed_after_label.set_label_color(Color::from_rgb(180, 180, 180));
    missed_after_label.set_label_font(Font::Courier);
    missed_after_label.set_label_size(12);
    missed_after_label.set_align(Align::Left | Align::Inside);

    // ========== Event Log ==========
    let mut log_header = Frame::default().with_size(880, 25);
    log_header.set_label("QoS Change Log");
    log_header.set_label_font(Font::HelveticaBold);
    log_header.set_label_size(14);
    log_header.set_label_color(Color::from_rgb(229, 192, 123));
    log_header.set_align(Align::Left | Align::Inside);

    let log_buffer = TextBuffer::default();
    let mut log_display = TextDisplay::default().with_size(880, 100);
    log_display.set_buffer(log_buffer.clone());
    log_display.set_text_font(Font::Courier);
    log_display.set_text_size(11);
    log_display.set_text_color(Color::from_rgb(180, 180, 180));
    log_display.set_color(Color::from_rgb(30, 33, 39));
    log_display.set_frame(FrameType::FlatBox);

    main_pack.end();
    wind.end();
    wind.show();

    // ========== Shared State ==========
    let state = Rc::new(RefCell::new(PanelState {
        deadline_ms: 100.0,
        latency_budget_ms: 10.0,
        transport_priority: 3.0,
        active_profile: "default".to_string(),
        log_buffer,
        sensor_labels,
        latency_before_label,
        latency_after_label,
        throughput_before_label,
        throughput_after_label,
        missed_before_label,
        missed_after_label,
    }));

    // Initial log entry
    {
        let mut s = state.borrow_mut();
        s.append_log("Panel started. Waiting for sensor data...");
    }

    // ========== Slider Callbacks ==========
    let state_d = state.clone();
    let mut deadline_label_c = deadline_label.clone();
    deadline_slider.set_callback(move |slider| {
        let val = slider.value();
        deadline_label_c.set_label(&format!("  Deadline (ms): {}", val as u64));
        state_d.borrow_mut().deadline_ms = val;
    });

    let state_l = state.clone();
    let mut latency_label_c = latency_label.clone();
    latency_slider.set_callback(move |slider| {
        let val = slider.value();
        latency_label_c.set_label(&format!("  Latency Budget (ms): {}", val as u64));
        state_l.borrow_mut().latency_budget_ms = val;
    });

    let state_p = state.clone();
    let mut priority_label_c = priority_label.clone();
    priority_slider.set_callback(move |slider| {
        let val = slider.value();
        priority_label_c.set_label(&format!("  Transport Priority: {}", val as i32));
        state_p.borrow_mut().transport_priority = val;
    });

    // ========== Profile Choice Callback ==========
    let state_pc = state.clone();
    let mut deadline_slider_c = deadline_slider.clone();
    let mut latency_slider_c = latency_slider.clone();
    let mut priority_slider_c = priority_slider.clone();
    let mut deadline_label_cc = deadline_label.clone();
    let mut latency_label_cc = latency_label.clone();
    let mut priority_label_cc = priority_label.clone();
    profile_choice.set_callback(move |choice| {
        let profile = match choice.value() {
            0 => ("default", 100.0, 10.0, 3.0),
            1 => ("combat", 10.0, 1.0, 7.0),
            2 => ("eco", 500.0, 50.0, 1.0),
            _ => ("default", 100.0, 10.0, 3.0),
        };

        let mut s = state_pc.borrow_mut();
        s.active_profile = profile.0.to_string();
        s.deadline_ms = profile.1;
        s.latency_budget_ms = profile.2;
        s.transport_priority = profile.3;

        deadline_slider_c.set_value(profile.1);
        latency_slider_c.set_value(profile.2);
        priority_slider_c.set_value(profile.3);

        deadline_label_cc.set_label(&format!("  Deadline (ms): {}", profile.1 as u64));
        latency_label_cc.set_label(&format!("  Latency Budget (ms): {}", profile.2 as u64));
        priority_label_cc.set_label(&format!("  Transport Priority: {}", profile.3 as i32));

        s.append_log(&format!("Switched to profile: {}", profile.0));
    });

    // ========== Apply Button Callback ==========
    let state_apply = state.clone();
    apply_btn.set_callback(move |_| {
        let mut s = state_apply.borrow_mut();

        let deadline = s.deadline_ms as u64;
        let latency = s.latency_budget_ms as u64;
        let priority = s.transport_priority as i32;

        let msg = format!(
            "Applying QoS: Deadline={}ms, LatencyBudget={}ms, Priority={}",
            deadline, latency, priority
        );
        s.append_log(&msg);

        match s.write_qos_yaml() {
            Ok(()) => {
                s.append_log("QoS profile written successfully - hot-reload will pick it up");
            }
            Err(e) => {
                s.append_log(&format!("ERROR: Failed to write QoS profile: {}", e));
            }
        }
    });

    // ========== Main Event Loop with DDS Polling ==========
    let sensor_names = ["Airspeed", "Altitude", "Heading", "Temperature"];
    let sensor_ids = ["airspeed", "altitude", "heading", "temperature"];

    while app.wait() {
        // Poll sensor reader
        while let Ok(Some(feed)) = sensor_reader.take() {
            let mut s = state.borrow_mut();
            for (i, sid) in sensor_ids.iter().enumerate() {
                if feed.sensor_id == *sid {
                    if i < s.sensor_labels.len() {
                        s.sensor_labels[i].set_label(&format!(
                            "  {} : {:.1}  (quality: {:.0}%)",
                            sensor_names[i],
                            feed.value,
                            feed.quality * 100.0
                        ));
                    }
                }
            }
        }

        // Poll metrics reader
        while let Ok(Some(delta)) = metrics_reader.take() {
            let mut s = state.borrow_mut();
            s.latency_before_label.set_label(&format!(
                "  Latency BEFORE: {:.1} us",
                delta.latency_before_us
            ));
            s.latency_after_label.set_label(&format!(
                "  Latency AFTER:  {:.1} us",
                delta.latency_after_us
            ));
            s.throughput_before_label.set_label(&format!(
                "  Throughput BEFORE: {:.1} msg/s",
                delta.throughput_before_mps
            ));
            s.throughput_after_label.set_label(&format!(
                "  Throughput AFTER:  {:.1} msg/s",
                delta.throughput_after_mps
            ));
            s.missed_before_label.set_label(&format!(
                "  Missed Deadlines BEFORE: {}",
                delta.missed_deadlines_before
            ));
            s.missed_after_label.set_label(&format!(
                "  Missed Deadlines AFTER:  {}",
                delta.missed_deadlines_after
            ));
            s.append_log("Metrics delta received - QoS change impact visible");
        }

        // Poll QoS snapshot reader
        while let Ok(Some(snap)) = snapshot_reader.take() {
            // Could update a QoS status display here
            let _ = snap; // consumed
        }

        app::sleep(0.05); // 20 Hz UI refresh
    }

    Ok(())
}
