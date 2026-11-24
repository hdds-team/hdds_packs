// SPDX-License-Identifier: Apache-2.0 OR MIT
// Protocol definitions for HC-12 communication with HDDS-Micro
//
// Frame format: [0xAA][0x55][LEN_HI][LEN_LO][RTPS...]
// RTPS contains: Header(20) + DATA Submessage(24+payload)
// CDR payload starts with message type byte

/// Sync bytes
pub const SYNC1: u8 = 0xAA;
pub const SYNC2: u8 = 0x55;

/// RTPS constants
pub const RTPS_MAGIC: [u8; 4] = [b'R', b'T', b'P', b'S'];
pub const RTPS_HEADER_SIZE: usize = 20;
pub const SUBMSG_DATA: u8 = 0x15;
// WORKAROUND: ESP32 hdds-micro lib sends 0x25 instead of 0x15
// TODO: Rebuild hdds-micro-c static library for ESP32 with correct value
pub const SUBMSG_DATA_ESP32_BUG: u8 = 0x25;

/// Message types (first byte of CDR payload)
pub const MSG_TYPE_TEMPERATURE: u8 = 0x00;  // Default/legacy temperature
pub const MSG_TYPE_IMAGE_START: u8 = 0x01;
pub const MSG_TYPE_IMAGE_DATA: u8 = 0x02;
pub const MSG_TYPE_IMAGE_END: u8 = 0x03;

/// Parsed temperature sample
#[derive(Debug, Clone)]
pub struct TemperatureSample {
    pub sensor_id: u32,
    pub value: f32,
    pub timestamp_ms: u64,
}

/// Image transfer start
#[derive(Debug, Clone)]
pub struct ImageStart {
    pub image_id: u32,
    pub total_size: u32,
    pub num_fragments: u16,
}

/// Image data fragment
#[derive(Debug, Clone)]
pub struct ImageData {
    pub image_id: u32,
    pub fragment_num: u16,
    pub payload: Vec<u8>,
}

/// Image transfer end
#[derive(Debug, Clone)]
pub struct ImageEnd {
    pub image_id: u32,
    pub checksum: u32,
}

/// Parsed message from sensor
#[derive(Debug, Clone)]
pub enum SensorMessage {
    Temperature(TemperatureSample),
    ImageStart(ImageStart),
    ImageData(ImageData),
    ImageEnd(ImageEnd),
}

pub enum ParseResult {
    Ok {
        message: SensorMessage,
        consumed: usize,
    },
    Incomplete,
    Error {
        consumed: usize,
    },
}

/// Parse framed RTPS packet
/// Supports two modes:
/// - Standard RTPS: full DATA submessage parsing
/// - Simple mode: direct CDR payload after RTPS header (for ESP32 compatibility)
pub fn parse_frame(data: &[u8]) -> ParseResult {
    // Find sync pattern AA 55
    let mut sync_pos = None;
    for i in 0..data.len().saturating_sub(1) {
        if data[i] == SYNC1 && data[i + 1] == SYNC2 {
            sync_pos = Some(i);
            break;
        }
    }

    match sync_pos {
        None => return ParseResult::Error { consumed: data.len().saturating_sub(1) },
        Some(0) => {}
        Some(n) => return ParseResult::Error { consumed: n },
    }

    // Need at least frame header
    if data.len() < 4 {
        return ParseResult::Incomplete;
    }

    // Get payload length (big-endian)
    let payload_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    let total_frame_len = 4 + payload_len;

    if data.len() < total_frame_len {
        return ParseResult::Incomplete;
    }

    let rtps_data = &data[4..4 + payload_len];

    // Verify RTPS magic
    if rtps_data.len() < RTPS_HEADER_SIZE || &rtps_data[0..4] != RTPS_MAGIC {
        let magic_bytes: Vec<String> = rtps_data.iter().take(24)
            .map(|b| format!("{:02X}", b))
            .collect();
        log::warn!("[PROTO] Invalid RTPS magic. First 24 bytes: {}", magic_bytes.join(" "));
        return ParseResult::Error { consumed: 1 };
    }

    // Debug: log RTPS header details and first 32 bytes
    let header_bytes: Vec<String> = rtps_data.iter().take(32)
        .map(|b| format!("{:02X}", b))
        .collect();
    log::debug!("[PROTO] RTPS frame len={}, first 32 bytes: {}", rtps_data.len(), header_bytes.join(" "));

    // Find DATA submessage (starts at offset 20)
    let submsg_start = RTPS_HEADER_SIZE;
    if rtps_data.len() < submsg_start + 4 {
        log::warn!("[PROTO] No submessage");
        return ParseResult::Error { consumed: total_frame_len };
    }

    let submsg_id = rtps_data[submsg_start];
    // Accept both correct 0x15 and ESP32's buggy 0x25
    let is_valid_submsg = submsg_id == SUBMSG_DATA || submsg_id == SUBMSG_DATA_ESP32_BUG;

    // For ESP32 compatibility, try parsing even if submessage ID doesn't match
    // The ESP32's RTPS encoding is buggy - try multiple strategies
    let cdr_offset = if is_valid_submsg {
        // Standard: DATA submessage header is 24 bytes
        submsg_start + 4 + 2 + 2 + 4 + 4 + 8
    } else {
        // ESP32 fallback: the DATA submessage structure might be corrupted
        // Try using a fixed offset based on typical message size
        // For a ~60 byte RTPS frame, payload should be near byte 44
        log::debug!("[PROTO] Using fallback parsing for submsg_id=0x{:02X}", submsg_id);
        submsg_start + 24  // Skip 24 bytes of DATA submsg header
    };

    if rtps_data.len() <= cdr_offset {
        return ParseResult::Error { consumed: total_frame_len };
    }

    let cdr_data = &rtps_data[cdr_offset..];

    // Parse based on message type (first byte)
    // Image messages have a type byte first (0x01, 0x02, 0x03)
    // Temperature messages start with sensor_id (u32) which is typically > 0x03

    if cdr_data.is_empty() {
        return ParseResult::Error { consumed: total_frame_len };
    }

    let first_byte = cdr_data[0];

    // Check for image message types first (0x01, 0x02, 0x03)
    if first_byte >= MSG_TYPE_IMAGE_START && first_byte <= MSG_TYPE_IMAGE_END {

        match first_byte {
            MSG_TYPE_IMAGE_START => {
                // Image start: type(1) + image_id(4) + total_size(4) + num_frags(2)
                if cdr_data.len() < 11 {
                    return ParseResult::Error { consumed: total_frame_len };
                }
                let image_id = u32::from_le_bytes([cdr_data[1], cdr_data[2], cdr_data[3], cdr_data[4]]);
                let total_size = u32::from_le_bytes([cdr_data[5], cdr_data[6], cdr_data[7], cdr_data[8]]);
                let num_fragments = u16::from_le_bytes([cdr_data[9], cdr_data[10]]);

                log::info!("[RX] IMAGE_START: id={} size={} frags={}", image_id, total_size, num_fragments);

                return ParseResult::Ok {
                    message: SensorMessage::ImageStart(ImageStart {
                        image_id,
                        total_size,
                        num_fragments,
                    }),
                    consumed: total_frame_len,
                };
            }
            MSG_TYPE_IMAGE_DATA => {
                // Image data: type(1) + image_id(4) + frag_num(2) + payload...
                if cdr_data.len() < 8 {
                    return ParseResult::Error { consumed: total_frame_len };
                }
                let image_id = u32::from_le_bytes([cdr_data[1], cdr_data[2], cdr_data[3], cdr_data[4]]);
                let fragment_num = u16::from_le_bytes([cdr_data[5], cdr_data[6]]);
                // Skip the length prefix from encode_bytes (4 bytes)
                let payload_start = 7;
                let payload = cdr_data[payload_start..].to_vec();

                if fragment_num % 50 == 0 {
                    log::info!("[RX] IMAGE_DATA: id={} frag={} len={}", image_id, fragment_num, payload.len());
                }

                return ParseResult::Ok {
                    message: SensorMessage::ImageData(ImageData {
                        image_id,
                        fragment_num,
                        payload,
                    }),
                    consumed: total_frame_len,
                };
            }
            MSG_TYPE_IMAGE_END => {
                // Image end: type(1) + image_id(4) + checksum(4)
                if cdr_data.len() < 9 {
                    return ParseResult::Error { consumed: total_frame_len };
                }
                let image_id = u32::from_le_bytes([cdr_data[1], cdr_data[2], cdr_data[3], cdr_data[4]]);
                let checksum = u32::from_le_bytes([cdr_data[5], cdr_data[6], cdr_data[7], cdr_data[8]]);

                log::info!("[RX] IMAGE_END: id={} checksum={:08X}", image_id, checksum);

                return ParseResult::Ok {
                    message: SensorMessage::ImageEnd(ImageEnd {
                        image_id,
                        checksum,
                    }),
                    consumed: total_frame_len,
                };
            }
            _ => {
                // Unknown image type
                return ParseResult::Error { consumed: total_frame_len };
            }
        }
    } else if cdr_data.len() >= 16 {
        // Temperature message: sensor_id(4) + value(4) + timestamp(8)
        let sensor_id = u32::from_le_bytes([cdr_data[0], cdr_data[1], cdr_data[2], cdr_data[3]]);
        let value = f32::from_le_bytes([cdr_data[4], cdr_data[5], cdr_data[6], cdr_data[7]]);
        let timestamp_ms = u64::from_le_bytes([
            cdr_data[8], cdr_data[9], cdr_data[10], cdr_data[11],
            cdr_data[12], cdr_data[13], cdr_data[14], cdr_data[15],
        ]);

        log::info!("[RX] TEMP: sensor=0x{:04X} T={:.2}°C ts={}ms", sensor_id, value, timestamp_ms);

        return ParseResult::Ok {
            message: SensorMessage::Temperature(TemperatureSample {
                sensor_id,
                value,
                timestamp_ms,
            }),
            consumed: total_frame_len,
        };
    }

    ParseResult::Error { consumed: total_frame_len }
}

/// Build ACK (not used with HDDS-Micro BEST_EFFORT)
pub fn build_ack(buffer: &mut [u8], seq: u32) -> usize {
    buffer[0] = 0xFF;
    buffer[1..5].copy_from_slice(&seq.to_le_bytes());
    5
}
