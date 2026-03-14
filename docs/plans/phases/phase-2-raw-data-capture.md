# Phase 2: Raw Data Capture + Debug Server Implementation Plan

**Goal:** Add an event bus, fixture capture tool, and embedded debug web server to the SBC service so we can record raw BLE bytes as test fixtures and inspect live data in a browser.

**Architecture:** Phase 1 established BLE scanning and node connection with console logging. Phase 2 introduces a `tokio::sync::broadcast` channel as an event bus — the scanner and connection emit typed events, which are consumed by a fixture writer (saves to JSON) and a debug server (streams to browser via WebSocket). The debug server is embedded in the same Rust binary using axum.

**Identity note:** Advertising events in this phase must carry both canonical device identity and advertisement family. Fixture capture is not just raw bytes; it must preserve enough context to distinguish direct probe advertisements, node repeated-probe advertisements, and node self-advertisements.

**Tech Stack (additions to Phase 1):** axum 0.7 (web server + WebSocket), serde + serde_json (serialization), clap 4 (CLI arguments)

**Prerequisite:** Phase 1 complete — `sbc-service/` builds, and the validation `scan` / `inspect` flows work on hardware.

**Reference docs:**

- `docs/plans/2026-02-21-testing-strategy-design.md` — Capture format specification and scenario list
- `docs/plans/2026-02-21-sbc-service-design.md` — Debug server architecture (axum, port 3001, WebSocket)
- `docs/plans/2026-02-21-tech-stack-review.md` — Debug app decision (embedded in Rust service)

---

## Project Structure (end state of this phase)

```
sbc-service/
├── Cargo.toml              # Updated with axum, serde, clap
├── src/
│   ├── main.rs              # Updated: CLI args, event bus, debug server spawn
│   ├── ble/
│   │   ├── mod.rs
│   │   ├── constants.rs
│   │   ├── scanner.rs       # Updated: emits BleEvent::Advertising
│   │   ├── connection.rs    # Updated: emits BleEvent::UartNotification
│   │   └── events.rs        # NEW: BleEvent enum, timestamp util, hex helpers
│   ├── capture.rs           # NEW: CaptureFile/CaptureEntry, fixture writer
│   ├── debug_server.rs      # NEW: axum router, WebSocket handler
│   └── types.rs
├── static/
│   └── debug.html           # NEW: Debug UI (embedded via include_str!)
test-fixtures/               # NEW: captured BLE data (at repo root)
```

---

## Task 1: Add Phase 2 Dependencies

**Files:**

- Modify: `sbc-service/Cargo.toml`

**Step 1: Update Cargo.toml**

Add the new dependencies:

```toml
# sbc-service/Cargo.toml
[package]
name = "sbc-service"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
axum = "0.7"
clap = { version = "4", features = ["derive"] }
env_logger = "0.11"
futures = "0.3"
log = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
```

**Step 2: Verify it compiles**

Run: `cd sbc-service && cargo build`
Expected: Compiles successfully.

**Step 3: Commit**

```bash
git add sbc-service/Cargo.toml
git commit -m "chore: add axum, serde, clap dependencies for Phase 2"
```

---

## Task 2: BLE Event Types

**Files:**

- Create: `sbc-service/src/ble/events.rs`
- Modify: `sbc-service/src/ble/mod.rs`

**Step 1: Write tests for BLE event serialization**

Create `sbc-service/src/ble/events.rs` with types and tests:

```rust
// sbc-service/src/ble/events.rs
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::types::ProductType;

/// Returns the current time as milliseconds since Unix epoch.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as u64
}

/// Convert a byte slice to a continuous hex string (e.g. "cafe0145").
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Convert a byte slice to a space-separated hex string (e.g. "ca fe 01 45").
pub fn bytes_to_hex_spaced(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Look up the human-readable name for a UART message type byte.
/// The high bit indicates response (1) vs request (0).
pub fn uart_message_type_name(msg_type: u8) -> &'static str {
    let base = msg_type & 0x7F;
    match base {
        0x01 => "SetProbeID",
        0x02 => "SetProbeColor",
        0x03 => "ReadSessionInfo",
        0x04 => "ReadLogs",
        0x05 => "SetPrediction",
        0x06 => "ReadOverTemp",
        0x07 => "ConfigFoodSafe",
        0x08 => "ResetFoodSafe",
        0x09 => "SetPowerMode",
        0x0A => "ResetThermometer",
        0x0B => "SetAlarms",
        0x0C => "SilenceAlarms",
        0x40 => "DeviceConnected",
        0x41 => "DeviceDisconnected",
        0x42 => "ReadNodeList",
        0x43 => "ReadTopology",
        0x44 => "ReadProbeList",
        0x45 => "ProbeStatus",
        0x46 => "ProbeFirmwareRev",
        0x47 => "ProbeHardwareRev",
        0x48 => "ProbeModelInfo",
        0x49 => "Heartbeat",
        0x4A => "AssociateNode",
        0x4B => "SyncThermometerList",
        _ => "Unknown",
    }
}

/// A BLE event emitted by the scanner or node connection.
/// Sent through the broadcast channel to consumers (capture writer, debug server).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "source")]
pub enum BleEvent {
    /// An advertising packet from a Combustion device.
    #[serde(rename = "advertising")]
    Advertising {
        timestamp_ms: u64,
        peripheral_handle: String,
        advertisement_family: String,
        product_type: String,
        serial_number: String,
        rssi: Option<i16>,
        raw_bytes_hex: String,
    },

    /// A UART TX notification from the connected MeatNet node.
    #[serde(rename = "uart_tx")]
    UartNotification {
        timestamp_ms: u64,
        raw_bytes_hex: String,
        /// e.g. "0x45" — extracted from the UART frame if sync bytes are present
        message_type: Option<String>,
        /// e.g. "ProbeStatus"
        message_type_name: Option<String>,
        byte_count: usize,
    },
}

impl BleEvent {
    /// Create an Advertising event from scanner data.
    pub fn advertising(
        peripheral_handle: bluer::Address,
        advertisement_family: &str,
        product_type: ProductType,
        serial_number: String,
        rssi: Option<i16>,
        raw_bytes: &[u8],
    ) -> Self {
        BleEvent::Advertising {
            timestamp_ms: now_ms(),
            peripheral_handle: peripheral_handle.to_string(),
            advertisement_family: advertisement_family.to_string(),
            product_type: product_type.slug().to_string(),
            serial_number,
            rssi,
            raw_bytes_hex: bytes_to_hex(raw_bytes),
        }
    }

    /// Create a UartNotification event from connection data.
    pub fn uart_notification(raw_bytes: &[u8]) -> Self {
        // Extract message type if we have a valid UART frame (sync bytes 0xCA 0xFE)
        let (msg_type, msg_name) = if raw_bytes.len() >= 5
            && raw_bytes[0] == 0xCA
            && raw_bytes[1] == 0xFE
        {
            let mt = raw_bytes[4];
            (
                Some(format!("0x{:02X}", mt)),
                Some(uart_message_type_name(mt).to_string()),
            )
        } else {
            (None, None)
        };

        BleEvent::UartNotification {
            timestamp_ms: now_ms(),
            raw_bytes_hex: bytes_to_hex(raw_bytes),
            message_type: msg_type,
            message_type_name: msg_name,
            byte_count: raw_bytes.len(),
        }
    }

    pub fn timestamp_ms(&self) -> u64 {
        match self {
            BleEvent::Advertising { timestamp_ms, .. } => *timestamp_ms,
            BleEvent::UartNotification { timestamp_ms, .. } => *timestamp_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_to_hex_empty() {
        assert_eq!(bytes_to_hex(&[]), "");
    }

    #[test]
    fn bytes_to_hex_formats_correctly() {
        assert_eq!(bytes_to_hex(&[0xCA, 0xFE, 0x01]), "cafe01");
    }

    #[test]
    fn bytes_to_hex_spaced_formats_correctly() {
        assert_eq!(bytes_to_hex_spaced(&[0xCA, 0xFE, 0x01]), "ca fe 01");
    }

    #[test]
    fn uart_message_type_name_known_types() {
        assert_eq!(uart_message_type_name(0x45), "ProbeStatus");
        assert_eq!(uart_message_type_name(0x49), "Heartbeat");
        assert_eq!(uart_message_type_name(0x04), "ReadLogs");
        assert_eq!(uart_message_type_name(0x42), "ReadNodeList");
    }

    #[test]
    fn uart_message_type_name_response_bit() {
        // Response messages have high bit set — the name lookup strips it
        assert_eq!(uart_message_type_name(0xC5), "ProbeStatus"); // 0x45 | 0x80
        assert_eq!(uart_message_type_name(0xC9), "Heartbeat"); // 0x49 | 0x80
    }

    #[test]
    fn uart_message_type_name_unknown() {
        assert_eq!(uart_message_type_name(0x7F), "Unknown");
    }

    #[test]
    fn uart_notification_extracts_message_type() {
        // Simulated UART frame: sync (CA FE) + CRC (2 bytes) + msg type 0x45
        let bytes = vec![0xCA, 0xFE, 0x00, 0x00, 0x45, 0x00, 0x00];
        let event = BleEvent::uart_notification(&bytes);
        match event {
            BleEvent::UartNotification {
                message_type,
                message_type_name,
                byte_count,
                ..
            } => {
                assert_eq!(message_type, Some("0x45".to_string()));
                assert_eq!(message_type_name, Some("ProbeStatus".to_string()));
                assert_eq!(byte_count, 7);
            }
            _ => panic!("expected UartNotification"),
        }
    }

    #[test]
    fn uart_notification_no_sync_bytes() {
        let bytes = vec![0x00, 0x01, 0x02];
        let event = BleEvent::uart_notification(&bytes);
        match event {
            BleEvent::UartNotification {
                message_type,
                message_type_name,
                ..
            } => {
                assert_eq!(message_type, None);
                assert_eq!(message_type_name, None);
            }
            _ => panic!("expected UartNotification"),
        }
    }

    #[test]
    fn advertising_event_serializes_with_source_tag() {
        let json = serde_json::to_value(BleEvent::Advertising {
            timestamp_ms: 1000,
            peripheral_handle: "AA:BB:CC:DD:EE:FF".to_string(),
            advertisement_family: "direct-probe".to_string(),
            product_type: "predictive-probe".to_string(),
            serial_number: "10005205".to_string(),
            rssi: Some(-65),
            raw_bytes_hex: "cafe01".to_string(),
        })
        .unwrap();

        assert_eq!(json["source"], "advertising");
        assert_eq!(json["timestamp_ms"], 1000);
        assert_eq!(json["raw_bytes_hex"], "cafe01");
    }

    #[test]
    fn uart_event_serializes_with_source_tag() {
        let json = serde_json::to_value(BleEvent::UartNotification {
            timestamp_ms: 2000,
            raw_bytes_hex: "cafe004500".to_string(),
            message_type: Some("0x45".to_string()),
            message_type_name: Some("ProbeStatus".to_string()),
            byte_count: 5,
        })
        .unwrap();

        assert_eq!(json["source"], "uart_tx");
        assert_eq!(json["message_type"], "0x45");
    }
}
```

**Step 2: Update BLE module**

```rust
// sbc-service/src/ble/mod.rs
pub mod connection;
pub mod constants;
pub mod events;
pub mod scanner;
```

**Step 3: Run tests to verify they pass**

Run: `cd sbc-service && cargo test`
Expected: All tests pass (Phase 1 tests + new event tests).

**Step 4: Commit**

```bash
git add sbc-service/src/ble/events.rs sbc-service/src/ble/mod.rs
git commit -m "feat: BLE event types with serialization and message type lookup"
```

---

## Task 3: Event Bus Integration

**Files:**

- Modify: `sbc-service/src/ble/scanner.rs`
- Modify: `sbc-service/src/ble/connection.rs`
- Modify: `sbc-service/src/main.rs`

This task modifies the Phase 1 scanner and connection to emit events through a broadcast channel instead of only logging. Console logging is preserved alongside event emission.

**Step 1: Update scanner to emit events**

Modify `run_scanner` to accept a broadcast sender and emit Advertising events:

The scanner example below shows a Linux backend implementation of the event-emission flow. If validation selects a different backend, keep the same event shape and identity fields while translating backend-specific types and scan APIs.

```rust
// sbc-service/src/ble/scanner.rs
use std::collections::HashMap;

use anyhow::Result;
use bluer::{Adapter, AdapterEvent, Address, Device};
use futures::{pin_mut, StreamExt};
use tokio::sync::broadcast;

use super::constants::COMBUSTION_VENDOR_ID;
use super::events::BleEvent;
use crate::types::ProductType;

/// Information about a discovered Combustion device.
#[derive(Debug)]
pub struct DiscoveredDevice {
    pub peripheral_handle: Address,
    pub advertisement_family: AdvertisementFamily,
    pub product_type: ProductType,
    pub serial_number: String,
    pub rssi: Option<i16>,
    pub raw_manufacturer_data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvertisementFamily {
    DirectProbe,
    NodeRepeatedProbe,
    NodeSelf,
}

impl AdvertisementFamily {
    pub fn slug(&self) -> &'static str {
        match self {
            AdvertisementFamily::DirectProbe => "direct-probe",
            AdvertisementFamily::NodeRepeatedProbe => "node-repeated-probe",
            AdvertisementFamily::NodeSelf => "node-self",
        }
    }
}

fn classify_advertisement_family(product_type: ProductType, data: &[u8]) -> Option<AdvertisementFamily> {
    match product_type {
        ProductType::PredictiveProbe if data.len() >= 22 => Some(AdvertisementFamily::NodeRepeatedProbe),
        ProductType::PredictiveProbe if data.len() >= 21 => Some(AdvertisementFamily::DirectProbe),
        ProductType::MeatNetRepeater
        | ProductType::GiantGrillGauge
        | ProductType::Display
        | ProductType::Booster if data.len() >= 11 => Some(AdvertisementFamily::NodeSelf),
        _ => None,
    }
}

fn parse_normalized_serial(advertisement_family: AdvertisementFamily, data: &[u8]) -> Option<String> {
    match advertisement_family {
        AdvertisementFamily::DirectProbe | AdvertisementFamily::NodeRepeatedProbe => {
            if data.len() < 5 {
                return None;
            }
            let serial = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
            Some(format!("{serial:08X}"))
        }
        AdvertisementFamily::NodeSelf => {
            if data.len() < 11 {
                return None;
            }
            let serial = std::str::from_utf8(&data[1..11]).ok()?;
            Some(serial.trim_end_matches('\0').to_string())
        }
    }
}

/// Parse a discovered bluer Device into a DiscoveredDevice if it's a Combustion device.
/// Returns None if the device doesn't have Combustion manufacturer data.
async fn parse_combustion_device(device: &Device) -> Option<DiscoveredDevice> {
    let manufacturer_data = device.manufacturer_data().await.ok()??;
    let data = manufacturer_data.get(&COMBUSTION_VENDOR_ID)?;

    if data.is_empty() {
        log::warn!(
            "Combustion device {} has empty manufacturer data",
            device.address(),
        );
        return None;
    }

    let product_type = ProductType::from_byte(data[0]);
    let advertisement_family = classify_advertisement_family(product_type, data)?;
    let serial_number = parse_normalized_serial(advertisement_family, data)?;
    let rssi = device.rssi().await.ok()?;

    Some(DiscoveredDevice {
        peripheral_handle: device.address(),
        advertisement_family,
        product_type,
        serial_number,
        rssi,
        raw_manufacturer_data: data.clone(),
    })
}

/// Run the passive BLE advertising scanner.
/// Discovers Combustion devices, emits BleEvent::Advertising through the event bus,
/// and calls `on_node_found` when a new node is discovered.
/// The BLE handle is transport-only; canonical identity remains
/// exact Combustion `product_type + serial_number`.
///
/// This function runs forever (until the adapter stream ends or an error occurs).
pub async fn run_scanner(
    adapter: &Adapter,
    event_tx: broadcast::Sender<BleEvent>,
    on_node_found: impl Fn(Address, &DiscoveredDevice),
) -> Result<()> {
    let discover = adapter.discover_devices().await?;
    pin_mut!(discover);

    let mut seen_devices: HashMap<Address, ProductType> = HashMap::new();

    log::info!("BLE advertising scanner started");

    while let Some(event) = discover.next().await {
        match event {
            AdapterEvent::DeviceAdded(addr) => {
                let device = adapter.device(addr)?;
                if let Some(discovered) = parse_combustion_device(&device).await {
                    let is_new = !seen_devices.contains_key(&addr);
                    seen_devices.insert(addr, discovered.product_type);

                    // Emit event (ignore send errors — no subscribers is OK)
                    let _ = event_tx.send(BleEvent::advertising(
                        discovered.peripheral_handle,
                        discovered.advertisement_family.slug(),
                        discovered.product_type,
                        discovered.serial_number.clone(),
                        discovered.rssi,
                        &discovered.raw_manufacturer_data,
                    ));

                    log::info!(
                        "Combustion device: handle={} family={:?} key={}:{} rssi={:?}",
                        addr,
                        discovered.advertisement_family,
                        discovered.product_type.slug(),
                        discovered.serial_number,
                        discovered.rssi,
                    );

                    if is_new
                        && discovered.advertisement_family == AdvertisementFamily::NodeSelf
                        && discovered.product_type.is_node()
                    {
                        on_node_found(addr, &discovered);
                    }
                }
            }
            AdapterEvent::DeviceRemoved(addr) => {
                if let Some(product_type) = seen_devices.remove(&addr) {
                    log::debug!(
                        "Device removed: handle={} type={:?}",
                        addr,
                        product_type
                    );
                }
            }
            _ => {}
        }
    }

    Ok(())
}
```

**Step 2: Update connection to emit events**

Modify `listen_uart_notifications` to accept a broadcast sender:

```rust
// sbc-service/src/ble/connection.rs
use anyhow::{anyhow, Context, Result};
use bluer::gatt::remote::Characteristic;
use bluer::{Adapter, Address, Device};
use futures::StreamExt;
use tokio::sync::broadcast;

use super::constants::{UART_SERVICE_UUID, UART_TX_UUID};
use super::events::{bytes_to_hex_spaced, uart_message_type_name, BleEvent};

/// An active connection to a MeatNet node with UART characteristics resolved.
pub struct NodeConnection {
    pub device: Device,
    pub tx_characteristic: Characteristic,
}

/// Connect to a MeatNet node and discover the UART service.
/// Returns a NodeConnection with the TX characteristic ready for notification subscription.
pub async fn connect_to_node(adapter: &Adapter, addr: Address) -> Result<NodeConnection> {
    let device = adapter.device(addr)?;

    if !device.is_connected().await? {
        log::info!("Connecting to node at {}...", addr);
        device.connect().await.context("Failed to connect")?;
    }
    log::info!("Connected to node at {}", addr);

    // Wait briefly for service discovery to complete
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Find the UART service
    let services = device
        .services()
        .await
        .context("Failed to enumerate services")?;
    log::debug!("Found {} services on node", services.len());

    let uart_service = services
        .into_iter()
        .find(|s| s.uuid().to_string() == UART_SERVICE_UUID)
        .ok_or_else(|| anyhow!("UART service not found on node at {}", addr))?;
    log::info!("Found UART service");

    // Find the TX characteristic (notifications from device)
    let characteristics = uart_service
        .characteristics()
        .await
        .context("Failed to enumerate characteristics")?;
    log::debug!(
        "Found {} characteristics on UART service",
        characteristics.len()
    );

    let tx_characteristic = characteristics
        .into_iter()
        .find(|c| c.uuid().to_string() == UART_TX_UUID)
        .ok_or_else(|| anyhow!("UART TX characteristic not found on node at {}", addr))?;
    log::info!("Found UART TX characteristic");

    Ok(NodeConnection {
        device,
        tx_characteristic,
    })
}

/// Subscribe to UART TX notifications, emit events, and log raw bytes.
/// Returns when the notification stream ends (device disconnected).
pub async fn listen_uart_notifications(
    connection: &NodeConnection,
    event_tx: broadcast::Sender<BleEvent>,
) -> Result<()> {
    log::info!("Subscribing to UART TX notifications...");
    let notify_stream = connection
        .tx_characteristic
        .notify()
        .await
        .context("Failed to subscribe to TX notifications")?;

    log::info!("Listening for UART TX notifications");
    futures::pin_mut!(notify_stream);

    while let Some(bytes) = notify_stream.next().await {
        // Emit event
        let _ = event_tx.send(BleEvent::uart_notification(&bytes));

        // Console logging
        if bytes.len() >= 5 && bytes[0] == 0xCA && bytes[1] == 0xFE {
            let msg_type = bytes[4];
            let is_response = msg_type & 0x80 != 0;
            let type_label = uart_message_type_name(msg_type);
            log::info!(
                "UART TX: type=0x{:02X} ({}{}) len={} bytes=[{}]",
                msg_type,
                if is_response { "Response:" } else { "" },
                type_label,
                bytes.len(),
                bytes_to_hex_spaced(&bytes)
            );
        } else {
            log::info!(
                "UART TX: len={} bytes=[{}]",
                bytes.len(),
                bytes_to_hex_spaced(&bytes)
            );
        }
    }

    log::warn!("UART TX notification stream ended (device disconnected?)");
    Ok(())
}

/// Calculate the backoff duration for a given attempt number.
/// Uses exponential backoff: 1s, 2s, 4s, 8s, 16s, capped at 30s.
pub fn backoff_duration(attempt: u32) -> std::time::Duration {
    let secs = 1u64.checked_shl(attempt).unwrap_or(30).min(30);
    std::time::Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn backoff_exponential_growth() {
        assert_eq!(backoff_duration(0), Duration::from_secs(1));
        assert_eq!(backoff_duration(1), Duration::from_secs(2));
        assert_eq!(backoff_duration(2), Duration::from_secs(4));
        assert_eq!(backoff_duration(3), Duration::from_secs(8));
        assert_eq!(backoff_duration(4), Duration::from_secs(16));
    }

    #[test]
    fn backoff_caps_at_30_seconds() {
        assert_eq!(backoff_duration(5), Duration::from_secs(30));
        assert_eq!(backoff_duration(6), Duration::from_secs(30));
        assert_eq!(backoff_duration(100), Duration::from_secs(30));
    }
}
```

**Step 3: Update main.rs to create the broadcast channel**

```rust
// sbc-service/src/main.rs
mod ble;
mod types;

use anyhow::Result;
use bluer::{Address, DiscoveryFilter};
use tokio::sync::{broadcast, mpsc};

use ble::events::BleEvent;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init();
    log::info!("SBC service starting");

    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    log::info!("Using Bluetooth adapter: {}", adapter.name());
    adapter.set_powered(true).await?;

    adapter
        .set_discovery_filter(DiscoveryFilter {
            transport: bluer::DiscoveryTransport::Le,
            ..Default::default()
        })
        .await?;

    // Event bus: broadcast channel for BLE events
    let (event_tx, _event_rx) = broadcast::channel::<BleEvent>(256);

    // Channel for the scanner to notify us when a node is found
    let (node_tx, mut node_rx) = mpsc::channel::<Address>(4);

    // Spawn the scanner
    let scanner_adapter = adapter.clone();
    let scanner_event_tx = event_tx.clone();
    tokio::spawn(async move {
        if let Err(e) =
            ble::scanner::run_scanner(&scanner_adapter, scanner_event_tx, move |addr, _device| {
                let _ = node_tx.try_send(addr);
            })
            .await
        {
            log::error!("Scanner error: {}", e);
        }
    });

    // Wait for the first node to be discovered
    log::info!("Scanning for MeatNet nodes... (Ctrl+C to stop)");
    let node_addr = tokio::select! {
        addr = node_rx.recv() => {
            addr.ok_or_else(|| anyhow::anyhow!("Scanner ended before finding a node"))?
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Shutting down (no node found)");
            return Ok(());
        }
    };
    log::info!("Found node at {}", node_addr);

    // Connection loop with reconnection
    let mut attempt: u32 = 0;
    loop {
        match ble::connection::connect_to_node(&adapter, node_addr).await {
            Ok(connection) => {
                attempt = 0;
                log::info!("Node connection established, listening for UART data...");

                tokio::select! {
                    result = ble::connection::listen_uart_notifications(&connection, event_tx.clone()) => {
                        if let Err(e) = result {
                            log::warn!("UART listener error: {}", e);
                        }
                        log::warn!("Node disconnected");
                    }
                    _ = tokio::signal::ctrl_c() => {
                        log::info!("Shutting down...");
                        let _ = connection.device.disconnect().await;
                        return Ok(());
                    }
                }
            }
            Err(e) => {
                log::warn!("Connection to node failed: {}", e);
            }
        }

        let backoff = ble::connection::backoff_duration(attempt);
        log::info!("Reconnecting in {:?} (attempt {})...", backoff, attempt + 1);

        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = tokio::signal::ctrl_c() => {
                log::info!("Shutting down during reconnect backoff");
                return Ok(());
            }
        }
        attempt += 1;
    }
}
```

**Step 4: Run tests and build**

Run: `cd sbc-service && cargo test && cargo build`
Expected: All tests pass, builds successfully.

**Step 5: Test on hardware**

Run: `RUST_LOG=info cargo run`
Expected: Same behavior as Phase 1 — devices discovered, node connected, UART bytes logged. The event bus is wired in but has no consumers yet (which is fine — broadcast ignores sends when there are no subscribers).

**Step 6: Commit**

```bash
git add sbc-service/src/ble/scanner.rs sbc-service/src/ble/connection.rs sbc-service/src/main.rs
git commit -m "feat: broadcast event bus for BLE events"
```

---

## Task 4: Fixture Format and Capture Writer

**Files:**

- Create: `sbc-service/src/capture.rs`
- Modify: `sbc-service/src/main.rs` (add `mod capture;`)

**Step 1: Write tests for the capture format**

Create `sbc-service/src/capture.rs`:

```rust
// sbc-service/src/capture.rs
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::ble::events::BleEvent;

/// A capture file matching the test fixture format from the testing strategy doc.
/// See: docs/plans/2026-02-21-testing-strategy-design.md
#[derive(Debug, Serialize, Deserialize)]
pub struct CaptureFile {
    pub scenario: String,
    pub description: String,
    pub devices: Vec<String>,
    pub captures: Vec<CaptureEntry>,
}

/// A single captured BLE event in the fixture format.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureEntry {
    pub timestamp: u64,
    pub source: String,
    pub raw_bytes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peripheral_handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advertisement_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl From<&BleEvent> for CaptureEntry {
    fn from(event: &BleEvent) -> Self {
        match event {
            BleEvent::Advertising {
                timestamp_ms,
                peripheral_handle,
                advertisement_family,
                product_type,
                serial_number,
                raw_bytes_hex,
                ..
            } => CaptureEntry {
                timestamp: *timestamp_ms,
                source: "advertising".to_string(),
                raw_bytes: raw_bytes_hex.clone(),
                message_type: None,
                peripheral_handle: Some(peripheral_handle.clone()),
                advertisement_family: Some(advertisement_family.clone()),
                serial_number: Some(serial_number.clone()),
                product_type: Some(product_type.clone()),
                note: None,
            },
            BleEvent::UartNotification {
                timestamp_ms,
                raw_bytes_hex,
                message_type,
                ..
            } => CaptureEntry {
                timestamp: *timestamp_ms,
                source: "uart_tx".to_string(),
                raw_bytes: raw_bytes_hex.clone(),
                message_type: message_type.clone(),
                peripheral_handle: None,
                advertisement_family: None,
                serial_number: None,
                product_type: None,
                note: None,
            },
        }
    }
}

/// Accumulates BLE events and writes them to a JSON fixture file.
pub struct CaptureWriter {
    scenario: String,
    description: String,
    output_path: PathBuf,
    entries: Vec<CaptureEntry>,
    devices_seen: Vec<String>,
}

impl CaptureWriter {
    pub fn new(scenario: String, description: String, output_dir: PathBuf) -> Self {
        let filename = format!("{}.json", scenario);
        let output_path = output_dir.join(filename);
        CaptureWriter {
            scenario,
            description,
            output_path,
            entries: Vec::new(),
            devices_seen: Vec::new(),
        }
    }

    /// Subscribe to the event bus and accumulate events until the channel closes.
    pub async fn run(&mut self, mut event_rx: broadcast::Receiver<BleEvent>) {
        log::info!("Capture writer started for scenario: {}", self.scenario);
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    // Track unique devices
                    if let BleEvent::Advertising {
                        product_type,
                        serial_number,
                        ..
                    } = &event
                    {
                        let device_id = format!("{product_type}:{serial_number}");
                        if !self.devices_seen.contains(&device_id) {
                            self.devices_seen.push(device_id);
                        }
                    }
                    self.entries.push(CaptureEntry::from(&event));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("Capture writer lagged, skipped {} events", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    log::info!("Event bus closed, stopping capture writer");
                    break;
                }
            }
        }
    }

    /// Write the accumulated capture to disk as JSON.
    pub fn write_to_file(&self) -> Result<PathBuf> {
        let capture_file = CaptureFile {
            scenario: self.scenario.clone(),
            description: self.description.clone(),
            devices: self.devices_seen.clone(),
            captures: self.entries.clone(),
        };

        // Ensure output directory exists
        if let Some(parent) = self.output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(&capture_file)?;
        std::fs::write(&self.output_path, &json)?;

        log::info!(
            "Capture written: {} entries to {}",
            self.entries.len(),
            self.output_path.display()
        );
        Ok(self.output_path.clone())
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_entry_from_advertising_event() {
        let event = BleEvent::Advertising {
            timestamp_ms: 1000,
            peripheral_handle: "AA:BB:CC:DD:EE:FF".to_string(),
            advertisement_family: "direct-probe".to_string(),
            product_type: "predictive-probe".to_string(),
            serial_number: "10005205".to_string(),
            rssi: Some(-65),
            raw_bytes_hex: "0105520010cafe".to_string(),
        };
        let entry = CaptureEntry::from(&event);
        assert_eq!(entry.source, "advertising");
        assert_eq!(entry.timestamp, 1000);
        assert_eq!(entry.raw_bytes, "0105520010cafe");
        assert_eq!(entry.serial_number, Some("10005205".to_string()));
        assert_eq!(
            entry.advertisement_family,
            Some("direct-probe".to_string())
        );
        assert_eq!(
            entry.peripheral_handle,
            Some("AA:BB:CC:DD:EE:FF".to_string())
        );
        assert!(entry.message_type.is_none());
    }

    #[test]
    fn capture_entry_from_uart_event() {
        let event = BleEvent::UartNotification {
            timestamp_ms: 2000,
            raw_bytes_hex: "cafe00450010".to_string(),
            message_type: Some("0x45".to_string()),
            message_type_name: Some("ProbeStatus".to_string()),
            byte_count: 6,
        };
        let entry = CaptureEntry::from(&event);
        assert_eq!(entry.source, "uart_tx");
        assert_eq!(entry.timestamp, 2000);
        assert_eq!(entry.message_type, Some("0x45".to_string()));
        assert!(entry.peripheral_handle.is_none());
    }

    #[test]
    fn capture_file_serializes_to_expected_format() {
        let file = CaptureFile {
            scenario: "test-scenario".to_string(),
            description: "A test".to_string(),
            devices: vec!["predictive-probe:AABBCCDD".to_string()],
            captures: vec![CaptureEntry {
                timestamp: 1000,
                source: "advertising".to_string(),
                raw_bytes: "cafe01".to_string(),
                message_type: None,
                peripheral_handle: Some("AA:BB:CC:DD:EE:FF".to_string()),
                advertisement_family: Some("direct-probe".to_string()),
                serial_number: Some("AABBCCDD".to_string()),
                product_type: Some("predictive-probe".to_string()),
                note: None,
            }],
        };
        let json = serde_json::to_string_pretty(&file).unwrap();
        assert!(json.contains("\"scenario\": \"test-scenario\""));
        assert!(json.contains("\"rawBytes\": \"cafe01\""));
        // Verify camelCase field names
        assert!(json.contains("rawBytes"));
        assert!(!json.contains("raw_bytes"));
    }

    #[test]
    fn capture_file_omits_none_fields() {
        let entry = CaptureEntry {
            timestamp: 1000,
            source: "uart_tx".to_string(),
            raw_bytes: "cafe".to_string(),
            message_type: Some("0x45".to_string()),
            peripheral_handle: None,
            advertisement_family: None,
            serial_number: None,
            product_type: None,
            note: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("peripheralHandle"));
        assert!(!json.contains("serialNumber"));
        assert!(!json.contains("note"));
        assert!(json.contains("messageType"));
    }

    #[test]
    fn capture_writer_writes_valid_json() {
        let dir = std::env::temp_dir().join("meatnet-test-capture");
        let mut writer = CaptureWriter::new(
            "test-write".to_string(),
            "test description".to_string(),
            dir.clone(),
        );

        // Manually add entries (bypassing the async run method)
        writer.entries.push(CaptureEntry {
            timestamp: 1000,
            source: "uart_tx".to_string(),
            raw_bytes: "cafe0045".to_string(),
            message_type: Some("0x45".to_string()),
            peripheral_handle: None,
            advertisement_family: None,
            serial_number: None,
            product_type: None,
            note: None,
        });
        writer
            .devices_seen
            .push("predictive-probe:AABBCCDD".to_string());

        let path = writer.write_to_file().unwrap();
        assert!(path.exists());

        let contents = std::fs::read_to_string(&path).unwrap();
        let parsed: CaptureFile = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed.scenario, "test-write");
        assert_eq!(parsed.captures.len(), 1);
        assert_eq!(parsed.devices.len(), 1);

        // Cleanup
        let _ = std::fs::remove_dir_all(dir);
    }
}
```

**Step 2: Add module to main.rs**

Add `mod capture;` to `sbc-service/src/main.rs` (just the module declaration, after `mod ble;`):

```rust
mod ble;
mod capture;
mod types;
```

**Step 3: Run tests**

Run: `cd sbc-service && cargo test`
Expected: All tests pass.

**Step 4: Commit**

```bash
git add sbc-service/src/capture.rs sbc-service/src/main.rs
git commit -m "feat: fixture capture writer with JSON format matching test strategy"
```

---

## Task 5: CLI Arguments + Capture Mode

**Files:**

- Modify: `sbc-service/src/main.rs`

**Step 1: Add CLI argument parsing and wire in capture writer**

Replace `main.rs` with the full updated version:

```rust
// sbc-service/src/main.rs
mod ble;
mod capture;
mod types;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use bluer::{Address, DiscoveryFilter};
use clap::Parser;
use tokio::sync::{broadcast, mpsc, Mutex};

use ble::events::BleEvent;
use capture::CaptureWriter;

#[derive(Parser)]
#[command(name = "sbc-service", about = "MeatNet companion SBC service")]
struct Cli {
    /// Enable capture mode: record all BLE data to a fixture file.
    /// Value is the scenario name (e.g. "probe-prediction-lifecycle").
    #[arg(long)]
    capture: Option<String>,

    /// Description for the capture file.
    #[arg(long, default_value = "")]
    capture_desc: String,

    /// Output directory for capture files.
    #[arg(long, default_value = "test-fixtures")]
    capture_dir: PathBuf,

    /// Port for the embedded debug web server.
    #[arg(long, default_value_t = 3001)]
    debug_port: u16,

    /// Disable the debug web server.
    #[arg(long)]
    no_debug_server: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    log::info!("SBC service starting");

    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    log::info!("Using Bluetooth adapter: {}", adapter.name());
    adapter.set_powered(true).await?;

    adapter
        .set_discovery_filter(DiscoveryFilter {
            transport: bluer::DiscoveryTransport::Le,
            ..Default::default()
        })
        .await?;

    // Event bus
    let (event_tx, _event_rx) = broadcast::channel::<BleEvent>(256);

    // Start capture writer if --capture is specified
    let capture_writer: Option<Arc<Mutex<CaptureWriter>>> = if let Some(ref scenario) = cli.capture
    {
        let writer = CaptureWriter::new(
            scenario.clone(),
            cli.capture_desc.clone(),
            cli.capture_dir.clone(),
        );
        let writer = Arc::new(Mutex::new(writer));
        let writer_clone = writer.clone();
        let capture_rx = event_tx.subscribe();
        tokio::spawn(async move {
            writer_clone.lock().await.run(capture_rx).await;
        });
        log::info!("Capture mode enabled for scenario: {}", scenario);
        Some(writer)
    } else {
        None
    };

    // Scanner
    let (node_tx, mut node_rx) = mpsc::channel::<Address>(4);
    let scanner_adapter = adapter.clone();
    let scanner_event_tx = event_tx.clone();
    tokio::spawn(async move {
        if let Err(e) =
            ble::scanner::run_scanner(&scanner_adapter, scanner_event_tx, move |addr, _device| {
                let _ = node_tx.try_send(addr);
            })
            .await
        {
            log::error!("Scanner error: {}", e);
        }
    });

    // Wait for node
    log::info!("Scanning for MeatNet nodes... (Ctrl+C to stop)");
    let node_addr = tokio::select! {
        addr = node_rx.recv() => {
            addr.ok_or_else(|| anyhow::anyhow!("Scanner ended before finding a node"))?
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Shutting down (no node found)");
            write_capture_on_shutdown(&capture_writer).await;
            return Ok(());
        }
    };
    log::info!("Found node at {}", node_addr);

    // Connection loop with reconnection
    let mut attempt: u32 = 0;
    loop {
        match ble::connection::connect_to_node(&adapter, node_addr).await {
            Ok(connection) => {
                attempt = 0;
                log::info!("Node connection established, listening for UART data...");

                tokio::select! {
                    result = ble::connection::listen_uart_notifications(&connection, event_tx.clone()) => {
                        if let Err(e) = result {
                            log::warn!("UART listener error: {}", e);
                        }
                        log::warn!("Node disconnected");
                    }
                    _ = tokio::signal::ctrl_c() => {
                        log::info!("Shutting down...");
                        let _ = connection.device.disconnect().await;
                        write_capture_on_shutdown(&capture_writer).await;
                        return Ok(());
                    }
                }
            }
            Err(e) => {
                log::warn!("Connection to node failed: {}", e);
            }
        }

        let backoff = ble::connection::backoff_duration(attempt);
        log::info!("Reconnecting in {:?} (attempt {})...", backoff, attempt + 1);

        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = tokio::signal::ctrl_c() => {
                log::info!("Shutting down during reconnect backoff");
                write_capture_on_shutdown(&capture_writer).await;
                return Ok(());
            }
        }
        attempt += 1;
    }
}

async fn write_capture_on_shutdown(writer: &Option<Arc<Mutex<CaptureWriter>>>) {
    if let Some(writer) = writer {
        let w = writer.lock().await;
        log::info!("Writing capture file ({} events)...", w.entry_count());
        match w.write_to_file() {
            Ok(path) => log::info!("Capture saved to {}", path.display()),
            Err(e) => log::error!("Failed to write capture file: {}", e),
        }
    }
}
```

**Step 2: Build and run tests**

Run: `cd sbc-service && cargo test && cargo build`
Expected: All tests pass, builds successfully.

**Step 3: Test CLI help**

Run: `cd sbc-service && cargo run -- --help`
Expected:

```
MeatNet companion SBC service

Usage: sbc-service [OPTIONS]

Options:
      --capture <CAPTURE>          Enable capture mode
      --capture-desc <CAPTURE_DESC>  Description for the capture file [default: ]
      --capture-dir <CAPTURE_DIR>  Output directory [default: test-fixtures]
      --debug-port <DEBUG_PORT>    Port for debug server [default: 3001]
      --no-debug-server            Disable the debug web server
  -h, --help                       Print help
```

**Step 4: Test capture mode on hardware**

Run on Raspberry Pi:

```bash
RUST_LOG=info cargo run -- --capture test-basic --capture-desc "Basic connectivity test"
```

Let it run for 30 seconds to collect some data, then press Ctrl+C.

Expected:

```
[INFO] Capture mode enabled for scenario: test-basic
...
[INFO] Shutting down...
[INFO] Writing capture file (47 events)...
[INFO] Capture saved to test-fixtures/test-basic.json
```

Verify the fixture file:

```bash
cat test-fixtures/test-basic.json | python3 -m json.tool | head -30
```

Expected: Valid JSON with `scenario`, `description`, `devices`, and `captures` array.

**Step 5: Commit**

```bash
git add sbc-service/src/main.rs
git commit -m "feat: CLI arguments with capture mode for recording test fixtures"
```

---

## Task 6: Debug Server

**Files:**

- Create: `sbc-service/src/debug_server.rs`
- Modify: `sbc-service/src/main.rs`

**Step 1: Create the debug server module**

```rust
// sbc-service/src/debug_server.rs
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use tokio::sync::broadcast;

use crate::ble::events::BleEvent;

struct AppState {
    event_tx: broadcast::Sender<BleEvent>,
}

/// Start the embedded debug web server.
/// Runs until the process exits.
pub async fn run(event_tx: broadcast::Sender<BleEvent>, port: u16) {
    let state = Arc::new(AppState { event_tx });

    let app = Router::new()
        .route("/debug", get(serve_debug_page))
        .route("/ws", get(ws_upgrade_handler))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    log::info!("Debug server listening on http://127.0.0.1:{}/debug", port);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind debug server port");
    axum::serve(listener, app)
        .await
        .expect("Debug server failed");
}

async fn serve_debug_page() -> impl IntoResponse {
    Html(include_str!("../static/debug.html"))
}

async fn ws_upgrade_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let event_rx = state.event_tx.subscribe();
    ws.on_upgrade(move |socket| handle_ws_connection(socket, event_rx))
}

async fn handle_ws_connection(
    mut socket: WebSocket,
    mut event_rx: broadcast::Receiver<BleEvent>,
) {
    log::info!("Debug WebSocket client connected");
    loop {
        match event_rx.recv().await {
            Ok(event) => {
                let json = match serde_json::to_string(&event) {
                    Ok(j) => j,
                    Err(e) => {
                        log::warn!("Failed to serialize event for WebSocket: {}", e);
                        continue;
                    }
                };
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break; // Client disconnected
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                log::warn!("WebSocket client lagged, skipped {} events", n);
            }
            Err(broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }
    log::info!("Debug WebSocket client disconnected");
}
```

**Step 2: Wire debug server into main.rs**

Add `mod debug_server;` to the module declarations in `main.rs`, and spawn the debug server after the event bus is created. Add the following right after the capture writer setup:

```rust
    // Start debug server (unless --no-debug-server)
    if !cli.no_debug_server {
        let debug_event_tx = event_tx.clone();
        let debug_port = cli.debug_port;
        tokio::spawn(async move {
            debug_server::run(debug_event_tx, debug_port).await;
        });
    }
```

Also add `mod debug_server;` at the top with the other module declarations:

```rust
mod ble;
mod capture;
mod debug_server;
mod types;
```

**Step 3: Create placeholder static file**

Create `sbc-service/static/debug.html` with a minimal placeholder:

```html
<!DOCTYPE html>
<html>
  <head>
    <title>MeatNet Debug</title>
  </head>
  <body>
    <h1>MeatNet Debug</h1>
    <p>UI coming in next task.</p>
  </body>
</html>
```

**Step 4: Build**

Run: `cd sbc-service && cargo build`
Expected: Compiles successfully.

**Step 5: Test on hardware**

Run: `RUST_LOG=info cargo run`

Expected:

```
[INFO] Debug server listening on http://127.0.0.1:3001/debug
```

Open `http://127.0.0.1:3001/debug` on the SBC — should show the placeholder page.
If LAN debug mode is explicitly enabled, use `http://<pi-ip>:3001/debug`.

**Step 6: Commit**

```bash
git add sbc-service/src/debug_server.rs sbc-service/src/main.rs sbc-service/static/debug.html
git commit -m "feat: embedded axum debug server with WebSocket event streaming"
```

---

## Task 7: Debug UI

**Files:**

- Modify: `sbc-service/static/debug.html`

**Step 1: Write the debug UI**

Replace `sbc-service/static/debug.html` with the full debug page:

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>MeatNet Debug</title>
    <style>
      * {
        margin: 0;
        padding: 0;
        box-sizing: border-box;
      }
      body {
        font-family: "Courier New", monospace;
        background: #1a1a2e;
        color: #e0e0e0;
        font-size: 13px;
      }
      header {
        background: #16213e;
        padding: 12px 20px;
        border-bottom: 1px solid #0f3460;
        display: flex;
        align-items: center;
        justify-content: space-between;
        position: sticky;
        top: 0;
        z-index: 10;
      }
      header h1 {
        font-size: 16px;
        color: #e94560;
      }
      .controls {
        display: flex;
        gap: 16px;
        align-items: center;
      }
      .controls label {
        cursor: pointer;
        user-select: none;
      }
      .controls button {
        background: #0f3460;
        color: #e0e0e0;
        border: 1px solid #e94560;
        padding: 4px 12px;
        cursor: pointer;
        font-family: inherit;
        font-size: 12px;
      }
      .controls button:hover {
        background: #e94560;
      }
      .status {
        padding: 6px 20px;
        background: #16213e;
        border-bottom: 1px solid #0f3460;
        font-size: 12px;
      }
      .status .connected {
        color: #4ecca3;
      }
      .status .disconnected {
        color: #e94560;
      }
      #events-container {
        overflow-y: auto;
        height: calc(100vh - 90px);
      }
      table {
        width: 100%;
        border-collapse: collapse;
      }
      thead {
        position: sticky;
        top: 0;
        background: #16213e;
        z-index: 5;
      }
      th {
        text-align: left;
        padding: 6px 10px;
        border-bottom: 2px solid #0f3460;
        color: #e94560;
        font-weight: normal;
        white-space: nowrap;
      }
      td {
        padding: 4px 10px;
        border-bottom: 1px solid #0f3460;
        vertical-align: top;
      }
      tr:hover {
        background: rgba(233, 69, 96, 0.1);
      }
      tr.advertising td.source-cell {
        color: #4ecdc4;
      }
      tr.uart_tx td.source-cell {
        color: #ffe66d;
      }
      .raw-bytes {
        word-break: break-all;
        max-width: 600px;
        color: #888;
        font-size: 11px;
      }
      .msg-type {
        font-weight: bold;
      }
      .time-col {
        white-space: nowrap;
        color: #666;
      }
      .count {
        color: #888;
      }
    </style>
  </head>
  <body>
    <header>
      <h1>MeatNet Debug</h1>
      <div class="controls">
        <label
          ><input type="checkbox" id="auto-scroll" checked /> Auto-scroll</label
        >
        <button id="pause-btn">Pause</button>
        <button id="clear-btn">Clear</button>
        <span class="count" id="count">0 events</span>
      </div>
    </header>
    <div class="status">
      WebSocket: <span id="ws-status" class="disconnected">connecting...</span>
    </div>
    <div id="events-container">
      <table>
        <thead>
          <tr>
            <th>Time</th>
            <th>Source</th>
            <th>Type</th>
            <th>Details</th>
            <th>Raw Bytes</th>
          </tr>
        </thead>
        <tbody id="events-body"></tbody>
      </table>
    </div>

    <script>
      const tbody = document.getElementById("events-body");
      const wsStatus = document.getElementById("ws-status");
      const countEl = document.getElementById("count");
      const autoScrollCb = document.getElementById("auto-scroll");
      const pauseBtn = document.getElementById("pause-btn");
      const clearBtn = document.getElementById("clear-btn");
      const container = document.getElementById("events-container");

      let eventCount = 0;
      let paused = false;
      let pendingEvents = [];
      const MAX_ROWS = 2000;

      function formatTime(ms) {
        const d = new Date(ms);
        return (
          d.toLocaleTimeString("en-US", { hour12: false }) +
          "." +
          String(d.getMilliseconds()).padStart(3, "0")
        );
      }

      function formatHex(hexStr) {
        // Insert spaces every 2 characters for readability
        return hexStr.replace(/(.{2})/g, "$1 ").trim();
      }

      function addEvent(event) {
        eventCount++;
        countEl.textContent = eventCount + " events";

        if (paused) {
          pendingEvents.push(event);
          countEl.textContent =
            eventCount + " events (" + pendingEvents.length + " buffered)";
          return;
        }

        appendRow(event);
      }

      function appendRow(event) {
        const tr = document.createElement("tr");
        tr.className = event.source;

        const isAdv = event.source === "advertising";
        const time = formatTime(event.timestamp_ms);
        const source = isAdv ? "ADV" : "UART";
        const msgType = isAdv
          ? ""
          : event.message_type_name || event.message_type || "";
        const details = isAdv
          ? (event.product_type || "") + " " + (event.serial_number || "")
          : (event.byte_count || "") + " bytes";
        const rawHex = formatHex(event.raw_bytes_hex || "");

        tr.innerHTML =
          '<td class="time-col">' +
          time +
          "</td>" +
          '<td class="source-cell">' +
          source +
          "</td>" +
          '<td class="msg-type">' +
          msgType +
          "</td>" +
          "<td>" +
          details +
          "</td>" +
          '<td class="raw-bytes">' +
          rawHex +
          "</td>";

        tbody.appendChild(tr);

        // Cap rows to prevent memory issues
        while (tbody.children.length > MAX_ROWS) {
          tbody.removeChild(tbody.firstChild);
        }

        if (autoScrollCb.checked) {
          container.scrollTop = container.scrollHeight;
        }
      }

      pauseBtn.addEventListener("click", function () {
        paused = !paused;
        pauseBtn.textContent = paused ? "Resume" : "Pause";
        if (!paused) {
          // Flush pending events
          pendingEvents.forEach(appendRow);
          pendingEvents = [];
          countEl.textContent = eventCount + " events";
        }
      });

      clearBtn.addEventListener("click", function () {
        tbody.innerHTML = "";
        eventCount = 0;
        pendingEvents = [];
        countEl.textContent = "0 events";
      });

      function connect() {
        const proto = location.protocol === "https:" ? "wss:" : "ws:";
        const ws = new WebSocket(proto + "//" + location.host + "/ws");

        ws.onopen = function () {
          wsStatus.textContent = "connected";
          wsStatus.className = "connected";
        };

        ws.onclose = function () {
          wsStatus.textContent = "disconnected (reconnecting...)";
          wsStatus.className = "disconnected";
          setTimeout(connect, 2000);
        };

        ws.onerror = function () {
          ws.close();
        };

        ws.onmessage = function (msg) {
          try {
            const event = JSON.parse(msg.data);
            addEvent(event);
          } catch (e) {
            console.error("Failed to parse event:", e);
          }
        };
      }

      connect();
    </script>
  </body>
</html>
```

**Step 2: Build**

Run: `cd sbc-service && cargo build`
Expected: Compiles (the HTML is embedded at compile time via `include_str!`).

**Step 3: Test on hardware**

Run: `RUST_LOG=info cargo run`

Open `http://127.0.0.1:3001/debug` in a browser on the SBC.
If LAN debug mode is explicitly enabled, use `http://<pi-ip>:3001/debug`.

Expected:

- WebSocket status shows "connected" in green
- Events stream in as they arrive:
  - ADV rows (teal) show product type and serial number
  - UART rows (yellow) show message type name (ProbeStatus, Heartbeat, etc.)
  - Raw bytes column shows space-separated hex
  - Event count increments
- Auto-scroll keeps the latest events visible
- Pause button buffers events and flushes on resume
- Clear button resets the view

**Step 4: Commit**

```bash
git add sbc-service/static/debug.html
git commit -m "feat: debug UI with live WebSocket event streaming"
```

---

## Task 8: Capture Key Scenarios

This task is manual — it uses the capture tool built in Tasks 1-7 to record real BLE data from Combustion devices. These fixture files become the test data for Phase 3 (protocol decoding).

**Files:**

- Create: `test-fixtures/` directory with captured JSON files

**Scenario list** (from `docs/plans/2026-02-21-testing-strategy-design.md`):

### Scenario 1: Probe idle (out of charger, not in food)

```bash
cd sbc-service
RUST_LOG=info cargo run -- \
    --capture probe-idle \
    --capture-desc "Probe out of charger, not inserted into food, normal mode"
```

With the probe out of the charger but not in food:

- Wait 60 seconds to collect steady-state data
- Press Ctrl+C

Expected fixture: advertising + ProbeStatus messages with stable room-temperature readings.

### Scenario 2: Probe inserted into food

```bash
RUST_LOG=info cargo run -- \
    --capture probe-inserted \
    --capture-desc "Probe inserted into food, temperature rising, no prediction set"
```

1. Start capture with probe at room temp
2. Insert probe into warm/hot food
3. Wait 60 seconds for temperatures to climb
4. Press Ctrl+C

Expected: temperature values change across captures, mode may change, virtual sensor assignments may shift.

### Scenario 3: Prediction lifecycle

```bash
RUST_LOG=info cargo run -- \
    --capture probe-prediction-lifecycle \
    --capture-desc "Probe with prediction set, warming through to prediction done"
```

1. Start capture
2. Insert probe into food
3. Set a prediction in the Combustion app (e.g., 95C removal)
4. Wait for prediction state to progress (warming → predicting)
5. If possible, let it reach "done" (may require a long cook)
6. Press Ctrl+C

Expected: prediction status fields change across ProbeStatus messages.

### Scenario 4: Multiple probes

```bash
RUST_LOG=info cargo run -- \
    --capture multi-probe \
    --capture-desc "Two or more probes active, interleaved advertising and ProbeStatus"
```

1. Have 2+ probes out of charger
2. Start capture
3. Wait 60 seconds
4. Press Ctrl+C

Expected: interleaved advertising packets and ProbeStatus messages from different probe serial numbers.

### Scenario 5: Heartbeat and topology

```bash
RUST_LOG=info cargo run -- \
    --capture heartbeat-topology \
    --capture-desc "Heartbeat and topology messages from connected node"
```

1. Start capture
2. Wait 2-3 minutes (heartbeats arrive periodically)
3. Press Ctrl+C

Expected: Heartbeat (0x49) messages in the UART stream. Topology messages (0x42/0x43) may appear if requested.

### Scenario 6: Node disconnect and reconnect

```bash
RUST_LOG=info cargo run -- \
    --capture node-reconnect \
    --capture-desc "Node powered off and back on to test disconnect/reconnect"
```

1. Start capture with stable connection
2. Wait 30 seconds for baseline data
3. Power off the node
4. Wait 15 seconds
5. Power the node back on
6. Wait for reconnection and data to resume
7. Press Ctrl+C

Expected: gap in UART messages during disconnect period, advertising may continue from probes in direct range.

### After capturing

Verify each fixture file is valid:

```bash
for f in test-fixtures/*.json; do
    echo "=== $f ==="
    python3 -c "import json; d=json.load(open('$f')); print(f'  scenario: {d[\"scenario\"]}'); print(f'  captures: {len(d[\"captures\"])}')"
done
```

Commit all fixtures:

```bash
git add test-fixtures/
git commit -m "feat: captured BLE test fixtures for Phase 3 protocol decoding"
```

---

## Verification Checklist (End of Phase 2)

1. **Unit tests pass:**

   ```bash
   cd sbc-service && cargo test
   ```

   Expected: All tests pass (Phase 1 + event types + capture format).

2. **Capture mode works:**

   ```bash
   RUST_LOG=info cargo run -- --capture quick-test --capture-desc "Verification test"
   ```

   Run for 30 seconds, Ctrl+C. Verify `test-fixtures/quick-test.json` exists and contains valid JSON.

3. **Debug server works:**

   ```bash
   RUST_LOG=info cargo run
   ```

   Open `http://127.0.0.1:3001/debug` on the SBC — verify events stream in the browser:
   if LAN debug mode is explicitly enabled, use `http://<pi-ip>:3001/debug`.
   - [ ] WebSocket connects and shows "connected"
   - [ ] ADV events appear with product type and serial
   - [ ] UART events appear with message type names
   - [ ] Raw bytes are displayed as hex
   - [ ] Auto-scroll works
   - [ ] Pause/Resume buffers and flushes events
   - [ ] Clear resets the view

4. **Capture fixtures exist** for at least 3 of the 6 scenarios:
   - [ ] `test-fixtures/probe-idle.json`
   - [ ] `test-fixtures/probe-inserted.json` (or similar)
   - [ ] `test-fixtures/heartbeat-topology.json` (or similar)

5. **CLI help is correct:**
   ```bash
   cargo run -- --help
   ```
   Shows all options with correct defaults.
Fixture identity rule:

- Use exact Combustion `product_type + serial_number` as the canonical device key in fixture metadata and grouping.
- Normalize probe-family serials as uppercase 8-character hex and node-family serials as protocol serial strings.
- If a BLE transport handle is captured for debugging, store it as `peripheral_handle` only.
- Never treat captured addresses or peripheral IDs as stable device identity.
