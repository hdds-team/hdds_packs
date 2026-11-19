// SPDX-License-Identifier: MIT
// HDDS Remote Sensor - Pi Zero 2 W Gateway
//
// Receives compact sensor data from HC-12 radio and publishes via full HDDS stack.
// Supports temperature samples and fragmented image transfers.

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use hdds::DDS;
use log::*;
use serial2::SerialPort;
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc;

mod protocol;
use protocol::{ParseResult, SensorMessage, TemperatureSample, ImageStart, ImageData};

// =============================================================================
// DDS TYPES
// =============================================================================

#[derive(Debug, Clone, DDS)]
struct Temperature {
    sensor_id: u32,
    value: f32,
    timestamp: u64,
}

#[derive(Debug, Clone, DDS)]
struct ImageMeta {
    image_id: u32,
    width: u16,
    height: u16,
    format: String,
    data: Vec<u8>,
}

// =============================================================================
// IMAGE REASSEMBLY
// =============================================================================

struct ImageAssembly {
    image_id: u32,
    total_size: u32,
    num_fragments: u16,
    fragments: HashMap<u16, Vec<u8>>,
    started_at: std::time::Instant,
}

impl ImageAssembly {
    fn new(start: &ImageStart) -> Self {
        Self {
            image_id: start.image_id,
            total_size: start.total_size,
            num_fragments: start.num_fragments,
            fragments: HashMap::with_capacity(start.num_fragments as usize),
            started_at: std::time::Instant::now(),
        }
    }

    fn add_fragment(&mut self, frag: &ImageData) -> bool {
        if frag.image_id != self.image_id {
            return false;
        }
        self.fragments.insert(frag.fragment_num, frag.payload.clone());
        true
    }

    fn is_complete(&self) -> bool {
        self.fragments.len() >= self.num_fragments as usize
    }

    fn assemble(&self) -> Option<Vec<u8>> {
        if !self.is_complete() {
            return None;
        }

        let mut data = Vec::with_capacity(self.total_size as usize);
        for i in 0..self.num_fragments {
            if let Some(frag) = self.fragments.get(&i) {
                data.extend_from_slice(frag);
            } else {
                warn!("[IMG] Missing fragment {} for image {}", i, self.image_id);
                return None;
            }
        }
        Some(data)
    }

    fn verify_checksum(&self, data: &[u8], expected: u32) -> bool {
        let computed: u32 = data.iter().map(|&b| b as u32).sum();
        if computed != expected {
            warn!("[IMG] Checksum mismatch: computed={:08X} expected={:08X}", computed, expected);
            false
        } else {
            true
        }
    }
}

/// Message types sent to DDS publisher task
enum DdsMessage {
    Temperature(TemperatureSample),
    Image { image_id: u32, data: Vec<u8> },
}

// =============================================================================
// CONFIGURATION
// =============================================================================

const SERIAL_PORT: &str = "/dev/serial0";  // HC-12 via GPIO14(TX)/GPIO15(RX)
const SERIAL_BAUD: u32 = 9600;
const WEB_PORT: u16 = 8080;
/// DDS domain for the demo packs, from the project-wide domain table.
const DDS_DOMAIN: u32 = 99;
const IMAGE_DIR: &str = "/tmp/hdds-images";  // Where received images are saved

// Static peer for unicast DDS (server IP:port)
// Set via HDDS_PEER environment variable
fn get_static_peer() -> Option<String> {
    std::env::var("HDDS_PEER").ok()
}

// TCP port for WAN communication
// Set via HDDS_TCP_PORT environment variable
fn get_tcp_port() -> Option<u16> {
    std::env::var("HDDS_TCP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
}

// QUIC transport for NAT traversal
// Set via HDDS_QUIC=1 environment variable
fn quic_enabled() -> bool {
    std::env::var("HDDS_QUIC").map(|v| v == "1").unwrap_or(false)
}

// LowBw transport preset (for future bidirectional HC-12 DDS)
// Set via HDDS_LOWBW_PRESET environment variable (e.g., "hc12", "lora")
fn get_lowbw_preset() -> Option<String> {
    std::env::var("HDDS_LOWBW_PRESET").ok()
}

// =============================================================================
// SHARED STATE
// =============================================================================

#[derive(Clone)]
struct SensorData {
    sensor_id: u32,
    temperature: f32,
    timestamp_ms: u64,
    rx_frames: u32,
    rx_errors: u32,
    bytes_received: u64,
    last_update: Option<std::time::Instant>,
    start_time: std::time::Instant,
    // Image stats
    images_received: u32,
    images_in_progress: u32,
    last_image_size: u32,
}

impl Default for SensorData {
    fn default() -> Self {
        Self {
            sensor_id: 0,
            temperature: 0.0,
            timestamp_ms: 0,
            rx_frames: 0,
            rx_errors: 0,
            bytes_received: 0,
            last_update: None,
            start_time: std::time::Instant::now(),
            images_received: 0,
            images_in_progress: 0,
            last_image_size: 0,
        }
    }
}

type SharedState = Arc<RwLock<SensorData>>;

// =============================================================================
// HC-12 RADIO TASK
// =============================================================================

async fn radio_task(
    port_name: &str,
    baud: u32,
    state: SharedState,
    tx: mpsc::Sender<DdsMessage>,
) -> Result<()> {
    info!("Opening serial port {} at {} baud...", port_name, baud);

    let mut port = SerialPort::open(port_name, baud)
        .context("Failed to open serial port")?;

    port.set_read_timeout(Duration::from_millis(100))?;

    info!("HC-12 radio ready (RTPS mode)");

    let mut rx_buf = vec![0u8; 512];  // Larger buffer for image fragments
    let mut rx_len = 0usize;

    // Image reassembly state
    let mut current_image: Option<ImageAssembly> = None;

    loop {
        // Read available data
        match port.read(&mut rx_buf[rx_len..]) {
            Ok(n) if n > 0 => {
                rx_len += n;
            }
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => {
                warn!("Serial read error: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        }

        // Parse frames
        while rx_len > 0 {
            match protocol::parse_frame(&rx_buf[..rx_len]) {
                ParseResult::Ok { message, consumed } => {
                    // Update stats
                    {
                        let mut data = state.write().unwrap();
                        data.rx_frames += 1;
                        data.bytes_received += consumed as u64;
                        data.last_update = Some(std::time::Instant::now());
                    }

                    // Handle message by type
                    match message {
                        SensorMessage::Temperature(sample) => {
                            {
                                let mut data = state.write().unwrap();
                                data.sensor_id = sample.sensor_id;
                                data.temperature = sample.value;
                                data.timestamp_ms = sample.timestamp_ms;
                            }
                            let _ = tx.send(DdsMessage::Temperature(sample)).await;
                        }

                        SensorMessage::ImageStart(start) => {
                            info!("[IMG] Starting image {}: {} bytes, {} fragments",
                                  start.image_id, start.total_size, start.num_fragments);

                            // Drop any incomplete image
                            if current_image.is_some() {
                                warn!("[IMG] Dropping incomplete image");
                            }
                            current_image = Some(ImageAssembly::new(&start));

                            let mut data = state.write().unwrap();
                            data.images_in_progress = 1;
                        }

                        SensorMessage::ImageData(frag) => {
                            if let Some(ref mut assembly) = current_image {
                                if assembly.add_fragment(&frag) {
                                    if frag.fragment_num % 50 == 0 {
                                        info!("[IMG] Fragment {}/{} for image {}",
                                              assembly.fragments.len(), assembly.num_fragments,
                                              assembly.image_id);
                                    }
                                }
                            } else {
                                warn!("[IMG] Received fragment without START");
                            }
                        }

                        SensorMessage::ImageEnd(end) => {
                            if let Some(assembly) = current_image.take() {
                                if assembly.image_id != end.image_id {
                                    warn!("[IMG] END image_id mismatch");
                                } else if let Some(data) = assembly.assemble() {
                                    if assembly.verify_checksum(&data, end.checksum) {
                                        info!("[IMG] Image {} complete: {} bytes in {:.1}s",
                                              end.image_id, data.len(),
                                              assembly.started_at.elapsed().as_secs_f32());

                                        // Update stats
                                        {
                                            let mut s = state.write().unwrap();
                                            s.images_received += 1;
                                            s.images_in_progress = 0;
                                            s.last_image_size = data.len() as u32;
                                        }

                                        // Send to DDS
                                        let _ = tx.send(DdsMessage::Image {
                                            image_id: end.image_id,
                                            data,
                                        }).await;
                                    } else {
                                        warn!("[IMG] Checksum failed for image {}", end.image_id);
                                    }
                                } else {
                                    warn!("[IMG] Failed to assemble image {}", end.image_id);
                                }

                                let mut s = state.write().unwrap();
                                s.images_in_progress = 0;
                            } else {
                                warn!("[IMG] Received END without START");
                            }
                        }
                    }

                    // Remove consumed bytes
                    rx_buf.copy_within(consumed..rx_len, 0);
                    rx_len -= consumed;
                }
                ParseResult::Incomplete => break,
                ParseResult::Error { consumed } => {
                    {
                        let mut data = state.write().unwrap();
                        data.rx_errors += 1;
                    }
                    let skip = consumed.max(1);
                    rx_buf.copy_within(skip..rx_len, 0);
                    rx_len = rx_len.saturating_sub(skip);
                }
            }
        }

        // Timeout incomplete images (60 seconds)
        if let Some(ref assembly) = current_image {
            if assembly.started_at.elapsed() > Duration::from_secs(60) {
                warn!("[IMG] Image {} timed out ({}/{} fragments)",
                      assembly.image_id, assembly.fragments.len(), assembly.num_fragments);
                current_image = None;
                let mut s = state.write().unwrap();
                s.images_in_progress = 0;
            }
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// =============================================================================
// DDS PUBLISHER TASK
// =============================================================================

async fn dds_publisher_task(
    _state: SharedState,
    mut rx: mpsc::Receiver<DdsMessage>,
) -> Result<()> {
    info!("Initializing HDDS on domain {}...", DDS_DOMAIN);

    // Create images directory
    let images_dir = std::path::Path::new(IMAGE_DIR);
    std::fs::create_dir_all(images_dir).context("Failed to create images directory")?;

    let mut builder = hdds::Participant::builder("hdds_remote_sensor")
        .domain_id(DDS_DOMAIN);

    // Configure transport based on environment
    let static_peer = get_static_peer();
    let tcp_port = get_tcp_port();

    match (&static_peer, tcp_port) {
        // TCP-only with initial_peers
        (Some(peer), Some(port)) => {
            info!("TCP-only mode: local port {}, peer {}", port, peer);
            let peer_addr: std::net::SocketAddr = peer.parse()
                .context("Invalid HDDS_PEER address")?;
            builder = builder.tcp_config(hdds::TcpConfig::tcp_only(vec![peer_addr]).with_port(port));
        }
        // UDP multicast with static peer for SPDP
        (Some(peer), None) => {
            info!("UDP multicast with static peer: {}", peer);
            builder = builder
                .with_transport(hdds::TransportMode::UdpMulticast)
                .add_static_peer(&peer);
        }
        // TCP only (no peer)
        (None, Some(port)) => {
            info!("TCP transport on port {}", port);
            builder = builder.with_tcp(port);
        }
        // Default: UDP multicast
        (None, None) => {
            info!("Using UDP multicast discovery");
            builder = builder.with_transport(hdds::TransportMode::UdpMulticast);
        }
    }

    // Enable QUIC transport for NAT traversal
    if quic_enabled() {
        info!("Enabling QUIC transport");
        builder = builder.with_quic(hdds::QuicConfig::default());
    }

    // Enable LowBw transport for constrained links
    if let Some(preset) = get_lowbw_preset() {
        info!("Enabling LowBw transport with preset: {}", preset);
        builder = builder.lowbw_preset(&preset);
    }

    let participant = builder
        .build()
        .context("Failed to create DDS participant")?;

    // Log TCP status
    if participant.tcp_enabled() {
        info!("TCP listening on {:?}", participant.tcp_listen_addr());
    }

    // Log QUIC status
    if participant.quic_enabled() {
        info!("QUIC transport enabled");
    }

    let qos = hdds::QoS::default();

    let temp_writer = participant
        .topic::<Temperature>("hdds_demo/remote_sensor/temperature")?
        .writer()
        .qos(qos.clone())
        .build()
        .context("Failed to create temperature writer")?;

    let image_writer = participant
        .topic::<ImageMeta>("hdds_demo/remote_sensor/image")?
        .writer()
        .qos(qos)
        .build()
        .context("Failed to create image writer")?;

    info!("HDDS publisher ready (temperature + images via DDS)");
    info!("Images also saved to: {}", IMAGE_DIR);

    while let Some(msg) = rx.recv().await {
        match msg {
            DdsMessage::Temperature(sample) => {
                info!("[DDS] Publishing temp: sensor=0x{:04X} T={:.2}°C ts={}",
                      sample.sensor_id, sample.value, sample.timestamp_ms);

                let temp = Temperature {
                    sensor_id: sample.sensor_id,
                    value: sample.value,
                    timestamp: sample.timestamp_ms,
                };

                if let Err(e) = temp_writer.write(&temp) {
                    warn!("Failed to publish temperature: {}", e);
                }
            }

            DdsMessage::Image { image_id, data } => {
                let filename = format!("image_{:08}.jpg", image_id);
                let path = images_dir.join(&filename);

                // Save image to disk
                if let Err(e) = std::fs::write(&path, &data) {
                    warn!("[IMG] Failed to save to disk: {}", e);
                }

                // Publish via DDS
                let image_meta = ImageMeta {
                    image_id,
                    width: 160,   // QQVGA
                    height: 120,
                    format: "jpeg".to_string(),
                    data: data.clone(),
                };

                match image_writer.write(&image_meta) {
                    Ok(_) => {
                        info!("[DDS] Published image: id={} size={} bytes", image_id, image_meta.data.len());
                    }
                    Err(e) => {
                        warn!("[DDS] Failed to publish image: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

// =============================================================================
// WEB SERVER
// =============================================================================

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../../web/index.html"))
}

async fn status_handler(State(state): State<SharedState>) -> impl IntoResponse {
    let data = state.read().unwrap();
    let ago = data.last_update
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(999);
    let gateway_uptime = data.start_time.elapsed().as_secs();
    let bitrate = if gateway_uptime > 0 {
        (data.bytes_received * 8) / gateway_uptime
    } else {
        0
    };

    let json = format!(
        r#"{{"sensor_id":"0x{:04X}","temperature":{:.2},"timestamp_ms":{},"rx_frames":{},"errors":{},"uptime":{},"bitrate":{},"last_update_ago":{},"images_received":{},"images_in_progress":{},"last_image_size":{}}}"#,
        data.sensor_id, data.temperature, data.timestamp_ms,
        data.rx_frames, data.rx_errors, gateway_uptime, bitrate, ago,
        data.images_received, data.images_in_progress, data.last_image_size
    );

    (StatusCode::OK, [("content-type", "application/json")], json)
}

async fn web_server(state: SharedState) -> Result<()> {
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/status", get(status_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", WEB_PORT);
    info!("Web server starting on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// =============================================================================
// MAIN
// =============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    info!("==========================================");
    info!("  HDDS Remote Sensor - Pi Gateway");
    info!("  RTPS over HC-12 @ 5V → Full HDDS");
    info!("==========================================");

    let state = Arc::new(RwLock::new(SensorData::default()));
    let (tx, rx) = mpsc::channel::<DdsMessage>(64);

    let state_clone = state.clone();
    let radio_handle = tokio::spawn(async move {
        if let Err(e) = radio_task(SERIAL_PORT, SERIAL_BAUD, state_clone, tx).await {
            error!("Radio task error: {}", e);
        }
    });

    let state_clone = state.clone();
    let dds_handle = tokio::spawn(async move {
        if let Err(e) = dds_publisher_task(state_clone, rx).await {
            error!("DDS task error: {}", e);
        }
    });

    let state_clone = state.clone();
    let web_handle = tokio::spawn(async move {
        if let Err(e) = web_server(state_clone).await {
            error!("Web server error: {}", e);
        }
    });

    info!("Gateway running!");
    info!("  Serial: {}", SERIAL_PORT);
    info!("  Web UI: http://localhost:{}", WEB_PORT);
    info!("  DDS Domain: {}", DDS_DOMAIN);

    let _ = tokio::join!(radio_handle, dds_handle, web_handle);

    Ok(())
}
