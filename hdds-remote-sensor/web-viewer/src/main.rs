// SPDX-License-Identifier: MIT
// HDDS Remote Sensor - Web Viewer
//
// Subscribes to DDS topics and serves a web UI for visualization.

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use hdds::DDS;
use log::*;
use serde::Serialize;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tower_http::cors::CorsLayer;

// =============================================================================
// DDS TYPES (must match pi-gateway)
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
// CONFIGURATION
// =============================================================================

const DDS_DOMAIN: u32 = 0;

fn get_web_port() -> u16 {
    std::env::var("WEB_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080)
}

// Static peer for unicast DDS (Pi gateway IP:port)
// Set via HDDS_PEER environment variable
fn get_static_peer() -> Option<String> {
    std::env::var("HDDS_PEER").ok()
}

// TCP port for WAN communication
// Set via HDDS_TCP_PORT environment variable (default: 7410)
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

// LowBw transport preset
// Set via HDDS_LOWBW_PRESET environment variable (e.g., "hc12", "lora")
fn get_lowbw_preset() -> Option<String> {
    std::env::var("HDDS_LOWBW_PRESET").ok()
}

// =============================================================================
// SHARED STATE
// =============================================================================

#[derive(Clone, Serialize)]
struct SensorStatus {
    sensor_id: String,
    temperature: f32,
    timestamp_ms: u64,
    node_online: bool,
    uptime: u64,
    rx_frames: u32,
    images_received: u32,
    last_image_id: u32,
    last_image_size: u32,
    bitrate: u64,
    errors: u32,
}

struct AppState {
    // Temperature data
    sensor_id: u32,
    temperature: f32,
    timestamp_ms: u64,

    // Image data
    last_image: Option<Vec<u8>>,
    last_image_id: u32,
    last_image_width: u16,
    last_image_height: u16,

    // Stats
    temp_count: u32,
    image_count: u32,
    error_count: u32,
    last_temp_time: Option<Instant>,
    last_image_time: Option<Instant>,
    start_time: Instant,
    bytes_received: u64,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sensor_id: 0,
            temperature: 0.0,
            timestamp_ms: 0,
            last_image: None,
            last_image_id: 0,
            last_image_width: 0,
            last_image_height: 0,
            temp_count: 0,
            image_count: 0,
            error_count: 0,
            last_temp_time: None,
            last_image_time: None,
            start_time: Instant::now(),
            bytes_received: 0,
        }
    }
}

type SharedState = Arc<RwLock<AppState>>;

// =============================================================================
// DDS SUBSCRIBER TASK
// =============================================================================

async fn dds_subscriber_task(state: SharedState) -> Result<()> {
    info!("Initializing HDDS subscriber on domain {}...", DDS_DOMAIN);

    let mut builder = hdds::Participant::builder("hdds_web_viewer")
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
        info!("QUIC enabled");
    }

    let qos = hdds::QoS::default();

    let temp_reader = participant
        .create_reader::<Temperature>("hdds_demo/remote_sensor/temperature", qos.clone())
        .context("Failed to create temperature reader")?;

    let image_reader = participant
        .create_reader::<ImageMeta>("hdds_demo/remote_sensor/image", qos)
        .context("Failed to create image reader")?;

    info!("HDDS subscriber ready");
    info!("  - Topic: hdds_demo/remote_sensor/temperature");
    info!("  - Topic: hdds_demo/remote_sensor/image");

    loop {
        // Poll temperature
        loop {
            match temp_reader.take() {
                Ok(Some(temp)) => {
                    info!("[DDS] Temperature: sensor=0x{:04X} T={:.2}°C ts={}",
                          temp.sensor_id, temp.value, temp.timestamp);

                    let mut s = state.write().unwrap();
                    s.sensor_id = temp.sensor_id;
                    s.temperature = temp.value;
                    s.timestamp_ms = temp.timestamp;
                    s.temp_count += 1;
                    s.last_temp_time = Some(Instant::now());
                    s.bytes_received += 16; // Approximate size
                }
                Ok(None) => break,
                Err(e) => {
                    warn!("[DDS] Temperature read error: {}", e);
                    let mut s = state.write().unwrap();
                    s.error_count += 1;
                    break;
                }
            }
        }

        // Poll images
        loop {
            match image_reader.take() {
                Ok(Some(img)) => {
                    info!("[DDS] Image: id={} {}x{} {} bytes",
                          img.image_id, img.width, img.height, img.data.len());

                    let mut s = state.write().unwrap();
                    s.bytes_received += img.data.len() as u64;
                    s.last_image_id = img.image_id;
                    s.last_image_width = img.width;
                    s.last_image_height = img.height;
                    s.last_image = Some(img.data);
                    s.image_count += 1;
                    s.last_image_time = Some(Instant::now());
                }
                Ok(None) => break,
                Err(e) => {
                    warn!("[DDS] Image read error: {}", e);
                    let mut s = state.write().unwrap();
                    s.error_count += 1;
                    break;
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// =============================================================================
// WEB HANDLERS
// =============================================================================

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../../web/index.html"))
}

async fn status_handler(State(state): State<SharedState>) -> Json<SensorStatus> {
    let s = state.read().unwrap();

    let uptime = s.start_time.elapsed().as_secs();
    let node_online = s.last_temp_time
        .map(|t| t.elapsed().as_secs() < 10)
        .unwrap_or(false);

    let bitrate = if uptime > 0 {
        (s.bytes_received * 8) / uptime
    } else {
        0
    };

    let last_image_size = s.last_image.as_ref().map(|v| v.len() as u32).unwrap_or(0);

    Json(SensorStatus {
        sensor_id: format!("0x{:04X}", s.sensor_id),
        temperature: s.temperature,
        timestamp_ms: s.timestamp_ms,
        node_online,
        uptime,
        rx_frames: s.temp_count,
        images_received: s.image_count,
        last_image_id: s.last_image_id,
        last_image_size,
        bitrate,
        errors: s.error_count,
    })
}

async fn image_handler(State(state): State<SharedState>) -> Response {
    let s = state.read().unwrap();

    match &s.last_image {
        Some(data) => {
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "image/jpeg")],
                data.clone(),
            ).into_response()
        }
        None => {
            (StatusCode::NOT_FOUND, "No image available").into_response()
        }
    }
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
    info!("  HDDS Remote Sensor - Web Viewer");
    info!("  Subscribe to DDS + Serve Web UI");
    info!("==========================================");

    let state = Arc::new(RwLock::new(AppState::default()));

    // Start DDS subscriber
    let state_clone = state.clone();
    let dds_handle = tokio::spawn(async move {
        if let Err(e) = dds_subscriber_task(state_clone).await {
            error!("DDS subscriber error: {}", e);
        }
    });

    // Start web server
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/status", get(status_handler))
        .route("/image.jpg", get(image_handler))
        .route("/api/image/latest.jpg", get(image_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let web_port = get_web_port();
    let addr = format!("0.0.0.0:{}", web_port);
    info!("Web server: http://{}", addr);
    info!("API status: http://{}/api/status", addr);
    info!("Live image: http://{}/image.jpg", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tokio::select! {
        _ = axum::serve(listener, app) => {}
        _ = dds_handle => {}
    }

    Ok(())
}
