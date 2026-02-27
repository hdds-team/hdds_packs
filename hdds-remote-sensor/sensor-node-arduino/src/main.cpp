/**
 * @file main.cpp
 * @brief HDDS Remote Sensor - ESP32-WROVER-CAM with HC-12
 *
 * Captures camera images and sends them via HDDS-Micro over HC-12 radio.
 * Uses EloquentEsp32Cam for stable camera handling.
 *
 * Hardware:
 *   - ESP32-WROVER-CAM (or ESP32-WROVER-DEV with OV2640)
 *   - HC-12 433MHz radio module (powered at 5V!)
 *
 * Copyright (c) 2025-2026 naskel.com
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include <Arduino.h>
#include <eloquent_esp32cam.h>
#include "hdds_micro.h"

using eloq::camera;

// =============================================================================
// CONFIGURATION
// =============================================================================

// HC-12 UART pins
#define HC12_TX_PIN 32
#define HC12_RX_PIN 33
#define HC12_BAUD   9600

// Node ID (unique per device)
#define NODE_ID 0x42

// Image capture interval (ms)
#define IMAGE_INTERVAL_MS 20000  // 20 seconds

// Temperature send interval (ms)
#define TEMP_INTERVAL_MS 2000

// Maximum payload per HDDS packet (HC-12 MTU is ~58 bytes, minus RTPS overhead)
#define MAX_PAYLOAD_SIZE 40

// =============================================================================
// GLOBAL STATE
// =============================================================================

HardwareSerial HC12Serial(2);  // UART2

HddsMicroTransport* g_transport = nullptr;
HddsMicroParticipant* g_participant = nullptr;
HddsMicroWriter* g_temp_writer = nullptr;
HddsMicroWriter* g_image_writer = nullptr;

uint32_t g_image_id = 0;

// =============================================================================
// UART CALLBACKS FOR HDDS TRANSPORT
// =============================================================================

int32_t uart_write(const uint8_t* data, size_t len, void* user_data) {
    if (len > 65535) return -1;

    // Debug: print first 32 bytes of RTPS data
    static uint32_t frame_count = 0;
    if (++frame_count <= 5) {  // Only first 5 frames
        Serial.printf("[UART] Frame #%lu, len=%d: ", frame_count, len);
        for (size_t i = 0; i < 32 && i < len; i++) {
            Serial.printf("%02X ", data[i]);
        }
        Serial.println();
    }

    uint8_t header[4];
    header[0] = 0xAA;
    header[1] = 0x55;
    header[2] = (len >> 8) & 0xFF;
    header[3] = len & 0xFF;

    HC12Serial.write(header, 4);
    HC12Serial.write(data, len);
    HC12Serial.flush();

    return len;
}

int32_t uart_read(uint8_t* buf, size_t max_len, uint32_t timeout_ms, void* user_data) {
    unsigned long start = millis();
    size_t received = 0;

    while (received < max_len) {
        if (HC12Serial.available()) {
            buf[received++] = HC12Serial.read();
        }
        if (timeout_ms > 0 && (millis() - start) >= timeout_ms) {
            break;
        }
        if (timeout_ms == 0 && !HC12Serial.available()) {
            break;
        }
        yield();
    }
    return received;
}

// =============================================================================
// HDDS INITIALIZATION
// =============================================================================

bool init_hdds() {
    g_transport = hdds_micro_transport_create_serial(
        uart_write, uart_read, NODE_ID, nullptr
    );
    if (!g_transport) {
        Serial.println("[ERROR] Failed to create transport");
        return false;
    }
    Serial.println("[OK] Transport created");

    g_participant = hdds_micro_participant_create(0, g_transport);
    if (!g_participant) {
        Serial.println("[ERROR] Failed to create participant");
        return false;
    }
    Serial.println("[OK] Participant created");

    g_temp_writer = hdds_micro_writer_create(
        g_participant, "hdds_demo/sensor/temperature", nullptr
    );
    if (!g_temp_writer) {
        Serial.println("[ERROR] Failed to create temperature writer");
        return false;
    }
    Serial.println("[OK] Temperature writer created");

    g_image_writer = hdds_micro_writer_create(
        g_participant, "hdds_demo/sensor/image", nullptr
    );
    if (!g_image_writer) {
        Serial.println("[ERROR] Failed to create image writer");
        return false;
    }
    Serial.println("[OK] Image writer created");

    return true;
}

// =============================================================================
// SEND TEMPERATURE
// =============================================================================

void send_temperature(float temp) {
    uint8_t buf[32];
    int32_t pos = 0;
    int32_t len;

    len = hdds_micro_encode_u32(buf + pos, sizeof(buf) - pos, NODE_ID);
    if (len < 0) return;
    pos += len;

    len = hdds_micro_encode_f32(buf + pos, sizeof(buf) - pos, temp);
    if (len < 0) return;
    pos += len;

    len = hdds_micro_encode_u64(buf + pos, sizeof(buf) - pos, millis());
    if (len < 0) return;
    pos += len;

    HddsMicroError err = hdds_micro_write(g_temp_writer, buf, pos);
    if (err == HDDS_MICRO_ERROR_OK) {
        Serial.printf("[TX] Temp: %.2f°C\n", temp);
    }
}

// =============================================================================
// SEND IMAGE (FRAGMENTED)
// =============================================================================

#define IMG_FRAME_START 0x01
#define IMG_FRAME_DATA  0x02
#define IMG_FRAME_END   0x03

void send_image(const uint8_t* jpeg_data, size_t jpeg_len) {
    g_image_id++;
    uint16_t num_fragments = (jpeg_len + MAX_PAYLOAD_SIZE - 1) / MAX_PAYLOAD_SIZE;

    Serial.printf("[IMG] Sending image #%lu: %d bytes, %d fragments\n",
                  g_image_id, jpeg_len, num_fragments);

    uint8_t buf[64];
    int32_t pos, len;

    // Send IMAGE_START
    pos = 0;
    len = hdds_micro_encode_u8(buf + pos, sizeof(buf) - pos, IMG_FRAME_START);
    pos += len;
    len = hdds_micro_encode_u32(buf + pos, sizeof(buf) - pos, g_image_id);
    pos += len;
    len = hdds_micro_encode_u32(buf + pos, sizeof(buf) - pos, jpeg_len);
    pos += len;
    len = hdds_micro_encode_u16(buf + pos, sizeof(buf) - pos, num_fragments);
    pos += len;

    hdds_micro_write(g_image_writer, buf, pos);
    Serial.printf("[IMG] START sent: id=%lu, size=%d, frags=%d (payload=%d bytes)\n",
                  g_image_id, jpeg_len, num_fragments, pos);
    delay(200);  // Longer delay after START to ensure HC-12 transmits

    // Send fragments
    size_t offset = 0;
    uint16_t frag_num = 0;

    while (offset < jpeg_len) {
        size_t chunk_size = min((size_t)MAX_PAYLOAD_SIZE, jpeg_len - offset);

        pos = 0;
        len = hdds_micro_encode_u8(buf + pos, sizeof(buf) - pos, IMG_FRAME_DATA);
        pos += len;
        len = hdds_micro_encode_u32(buf + pos, sizeof(buf) - pos, g_image_id);
        pos += len;
        len = hdds_micro_encode_u16(buf + pos, sizeof(buf) - pos, frag_num);
        pos += len;
        len = hdds_micro_encode_bytes(buf + pos, sizeof(buf) - pos, jpeg_data + offset, chunk_size);
        pos += len;

        hdds_micro_write(g_image_writer, buf, pos);

        if (frag_num % 50 == 0) {
            Serial.printf("[IMG] Fragment %d/%d\n", frag_num, num_fragments);
        }

        offset += chunk_size;
        frag_num++;
        delay(80);
    }

    // Send IMAGE_END
    pos = 0;
    len = hdds_micro_encode_u8(buf + pos, sizeof(buf) - pos, IMG_FRAME_END);
    pos += len;
    len = hdds_micro_encode_u32(buf + pos, sizeof(buf) - pos, g_image_id);
    pos += len;

    uint32_t checksum = 0;
    for (size_t i = 0; i < jpeg_len; i++) {
        checksum += jpeg_data[i];
    }
    len = hdds_micro_encode_u32(buf + pos, sizeof(buf) - pos, checksum);
    pos += len;

    hdds_micro_write(g_image_writer, buf, pos);
    Serial.printf("[IMG] END sent: id=%lu, checksum=%08lX\n", g_image_id, checksum);
}

// =============================================================================
// SETUP
// =============================================================================

void setup() {
    Serial.begin(115200);
    delay(3000);  // Comme dans ton code stable

    Serial.println();
    Serial.println("==========================================");
    Serial.println("  HDDS Remote Sensor - ESP32-WROVER-CAM");
    Serial.printf("  HDDS Micro SDK: %s\n", hdds_micro_version());
    Serial.println("==========================================");

    // Initialize HC-12 UART
    HC12Serial.begin(HC12_BAUD, SERIAL_8N1, HC12_RX_PIN, HC12_TX_PIN);
    Serial.printf("[OK] HC-12 UART initialized (TX=%d, RX=%d, %d baud)\n",
                  HC12_TX_PIN, HC12_RX_PIN, HC12_BAUD);

    // Camera config - exactly like your working code
    camera.pinout.wrover();
    camera.brownout.disable();
    camera.resolution.qvga();  // 320x240
    camera.quality.low();

    // Init camera
    while (!camera.begin().isOk()) {
        Serial.println(camera.exception.toString());
        delay(1000);
    }
    Serial.println("[OK] Camera initialized (QVGA 320x240)");

    // Initialize HDDS
    if (!init_hdds()) {
        Serial.println("[ERROR] HDDS init failed, halting");
        while (1) delay(1000);
    }

    Serial.println();
    Serial.println("Ready! Sending data...");
    Serial.println();
}

// =============================================================================
// LOOP
// =============================================================================

void loop() {
    static unsigned long last_temp = 0;
    static unsigned long last_image = 0;
    static uint32_t temp_seq = 0;

    unsigned long now = millis();

    // Send temperature every 2 seconds
    if (now - last_temp >= TEMP_INTERVAL_MS) {
        last_temp = now;
        temp_seq++;
        float temp = 20.0 + (temp_seq % 100) * 0.1;
        send_temperature(temp);
    }

    // Capture and send image every 20 seconds
    if (now - last_image >= IMAGE_INTERVAL_MS) {
        last_image = now;

        if (camera.capture().isOk()) {
            Serial.printf("[CAM] Captured: %dx%d, %d bytes\n",
                          camera.frame->width, camera.frame->height, camera.frame->len);
            send_image(camera.frame->buf, camera.frame->len);
        } else {
            Serial.print("[CAM] ");
            Serial.println(camera.exception.toString());
        }
    }

    delay(10);
}
