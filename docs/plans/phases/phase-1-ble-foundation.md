# Phase 1: BLE Foundation Implementation Plan

**Goal:** Establish BLE connectivity to Combustion Inc devices — scan for advertising packets, connect to a MeatNet node via GATT, and receive raw UART bytes.

**Architecture:** The SBC service is a Rust async application using bluer (BlueZ D-Bus interface) for all BLE operations. It runs a passive advertising scanner and a GATT connection to one MeatNet node concurrently using tokio. Raw bytes are logged to console for verification. This phase produces no parsed data — just raw bytes flowing from hardware through BLE to the console.

**Tech Stack:** Rust, bluer 0.17, tokio 1.x, futures 0.3, anyhow, env_logger. Though, check to see if there are newer versions and use those instead.

**Reference docs:**

- `external-docs/probe_ble_specification.rst` — Probe advertising format, GATT services, UART messages
- `external-docs/meatnet_node_ble_specification.rst` — Node advertising format, GATT services, UART messages (node headers are different from probe headers)
- `docs/plans/2026-02-21-sbc-service-design.md` — SBC service internal architecture
- `docs/plans/2026-02-21-meatnet-companion-design.md` — System architecture overview

**Key BLE facts from specs:**

- Combustion vendor ID: `0x09C7` (manufacturer data key in advertising)
- Manufacturer data is 24 bytes: vendor ID (2) + product type (1) + serial (4) + raw temps (13) + mode/ID (1) + battery/virtual (1) + network info (1) + overheating (1)
- Product types: 0=Unknown, 1=Predictive Probe, 2=MeatNet Repeater, 3=Giant Grill Gauge, 4=Display, 5=Booster
- Node UART service UUID: `6E400001-B5A3-F393-E0A9-E50E24DCCA9E`
- Node UART RX (write to device): `6E400002-B5A3-F393-E0A9-E50E24DCCA9E`
- Node UART TX (notifications from device): `6E400003-B5A3-F393-E0A9-E50E24DCCA9E`
- Node UART request header: 10 bytes (sync `0xCAFE`, CRC, msg type, request ID u32, payload len)
- Node UART response header: 15 bytes (sync, CRC, msg type, request ID u32, response ID u32, success, payload len)
- Key unsolicited messages on UART TX: Probe Status `0x45`, Heartbeat `0x49`

**Verification hardware required:** Raspberry Pi with Bluetooth, at least one Combustion Inc Predictive Probe, at least one MeatNet Node (Repeater/Display/Booster)

---

## Project Structure (end state of this phase)

```
sbc-service/
├── Cargo.toml
├── src/
│   ├── main.rs              # Application entry point and main loop
│   ├── ble/
│   │   ├── mod.rs            # Module declarations
│   │   ├── constants.rs      # BLE UUIDs, vendor ID, service constants
│   │   ├── scanner.rs        # Passive advertising scanner
│   │   └── connection.rs     # GATT connection to node + UART subscription
│   └── types.rs              # Domain types (ProductType)
```

---

## Task 1: Project Scaffold

**Files:**

- Modify: `.gitignore`
- Create: `sbc-service/Cargo.toml`
- Create: `sbc-service/src/main.rs`

**Step 1: Update .gitignore**

Add the Rust build directory:

```
# .gitignore (append to existing)
sbc-service/target/
```

**Step 2: Create Cargo.toml**

```toml
# sbc-service/Cargo.toml
[package]
name = "sbc-service"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
bluer = { version = "0.17", features = ["full"] }
env_logger = "0.11"
futures = "0.3"
log = "0.4"
tokio = { version = "1", features = ["full"] }
```

**Step 3: Create minimal main.rs**

```rust
// sbc-service/src/main.rs
use anyhow::Result;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init();
    log::info!("SBC service starting");
    Ok(())
}
```

**Step 4: Build and verify**

Run: `cd sbc-service && cargo build`
Expected: Compiles successfully with no errors.

**Step 5: Commit**

```bash
git add .gitignore sbc-service/Cargo.toml sbc-service/src/main.rs
git commit -m "feat: scaffold sbc-service Rust project"
```

---

## Task 2: Domain Types

**Files:**

- Create: `sbc-service/src/types.rs`
- Modify: `sbc-service/src/main.rs`

**Step 1: Write tests for ProductType**

Create `sbc-service/src/types.rs` with tests first:

```rust
// sbc-service/src/types.rs

/// Combustion Inc product types from BLE advertising data.
/// Byte 0 of manufacturer specific data (after vendor ID).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProductType {
    Unknown,
    PredictiveProbe,
    MeatNetRepeater,
    GiantGrillGauge,
    Display,
    Booster,
}

impl ProductType {
    /// Parse product type from the raw byte in advertising data.
    pub fn from_byte(byte: u8) -> Self {
        todo!()
    }

    /// Returns true if this device type is a MeatNet node (can be used as a gateway).
    pub fn is_node(&self) -> bool {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_byte_known_types() {
        assert_eq!(ProductType::from_byte(0), ProductType::Unknown);
        assert_eq!(ProductType::from_byte(1), ProductType::PredictiveProbe);
        assert_eq!(ProductType::from_byte(2), ProductType::MeatNetRepeater);
        assert_eq!(ProductType::from_byte(3), ProductType::GiantGrillGauge);
        assert_eq!(ProductType::from_byte(4), ProductType::Display);
        assert_eq!(ProductType::from_byte(5), ProductType::Booster);
    }

    #[test]
    fn from_byte_unknown_values() {
        assert_eq!(ProductType::from_byte(6), ProductType::Unknown);
        assert_eq!(ProductType::from_byte(255), ProductType::Unknown);
    }

    #[test]
    fn is_node_returns_true_for_gateway_devices() {
        assert!(ProductType::MeatNetRepeater.is_node());
        assert!(ProductType::Display.is_node());
        assert!(ProductType::Booster.is_node());
    }

    #[test]
    fn is_node_returns_false_for_non_gateway_devices() {
        assert!(!ProductType::PredictiveProbe.is_node());
        assert!(!ProductType::GiantGrillGauge.is_node());
        assert!(!ProductType::Unknown.is_node());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cd sbc-service && cargo test`
Expected: FAIL — `not yet implemented` panics from `todo!()`

**Step 3: Implement ProductType**

Replace the `todo!()` calls:

```rust
impl ProductType {
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            1 => ProductType::PredictiveProbe,
            2 => ProductType::MeatNetRepeater,
            3 => ProductType::GiantGrillGauge,
            4 => ProductType::Display,
            5 => ProductType::Booster,
            _ => ProductType::Unknown,
        }
    }

    pub fn is_node(&self) -> bool {
        matches!(
            self,
            ProductType::MeatNetRepeater | ProductType::Display | ProductType::Booster
        )
    }
}
```

**Step 4: Wire module into main.rs**

Add `mod types;` to main.rs:

```rust
// sbc-service/src/main.rs
mod types;

use anyhow::Result;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init();
    log::info!("SBC service starting");
    Ok(())
}
```

**Step 5: Run tests to verify they pass**

Run: `cd sbc-service && cargo test`
Expected: All 4 tests pass.

**Step 6: Commit**

```bash
git add sbc-service/src/types.rs sbc-service/src/main.rs
git commit -m "feat: add ProductType domain type with tests"
```

---

## Task 3: BLE Constants

**Files:**

- Create: `sbc-service/src/ble/mod.rs`
- Create: `sbc-service/src/ble/constants.rs`
- Modify: `sbc-service/src/main.rs`

**Step 1: Create the BLE module with constants**

```rust
// sbc-service/src/ble/mod.rs
pub mod constants;
```

```rust
// sbc-service/src/ble/constants.rs

/// Combustion Inc Bluetooth Company ID.
/// Used to filter advertising packets — only devices with manufacturer
/// data key 0x09C7 are Combustion devices.
pub const COMBUSTION_VENDOR_ID: u16 = 0x09C7;

/// Nordic UART Service UUID.
/// Both probes and nodes expose this service for bidirectional communication.
pub const UART_SERVICE_UUID: &str = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";

/// UART RX Characteristic UUID (write to device).
/// We write command bytes here to send commands through the node to probes.
pub const UART_RX_UUID: &str = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";

/// UART TX Characteristic UUID (notifications from device).
/// We subscribe to notifications here to receive Probe Status, Heartbeat,
/// and command response messages.
pub const UART_TX_UUID: &str = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";
```

**Step 2: Wire BLE module into main.rs**

```rust
// sbc-service/src/main.rs
mod ble;
mod types;

use anyhow::Result;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init();
    log::info!("SBC service starting");
    Ok(())
}
```

**Step 3: Verify it compiles**

Run: `cd sbc-service && cargo build`
Expected: Compiles successfully.

**Step 4: Commit**

```bash
git add sbc-service/src/ble/mod.rs sbc-service/src/ble/constants.rs sbc-service/src/main.rs
git commit -m "feat: add BLE constants (vendor ID, UART UUIDs)"
```

---

## Task 4: Advertising Scanner

**Files:**

- Create: `sbc-service/src/ble/scanner.rs`
- Modify: `sbc-service/src/ble/mod.rs`
- Modify: `sbc-service/src/main.rs`

This is the first hardware-dependent code. It cannot be unit tested — verification is done by running on the Raspberry Pi with Combustion devices nearby.

**Step 1: Create the scanner module**

```rust
// sbc-service/src/ble/scanner.rs
use std::collections::HashMap;

use anyhow::Result;
use bluer::{Adapter, AdapterEvent, Address, Device};
use futures::{pin_mut, StreamExt};

use super::constants::COMBUSTION_VENDOR_ID;
use crate::types::ProductType;

/// Information about a discovered Combustion device.
#[derive(Debug)]
pub struct DiscoveredDevice {
    pub address: Address,
    pub product_type: ProductType,
    pub serial_number: u32,
    pub rssi: Option<i16>,
    pub raw_manufacturer_data: Vec<u8>,
}

/// Parse a discovered bluer Device into a DiscoveredDevice if it's a Combustion device.
/// Returns None if the device doesn't have Combustion manufacturer data.
async fn parse_combustion_device(device: &Device) -> Option<DiscoveredDevice> {
    let manufacturer_data = device.manufacturer_data().await.ok()??;
    let data = manufacturer_data.get(&COMBUSTION_VENDOR_ID)?;

    if data.len() < 22 {
        log::warn!(
            "Combustion device {} has short manufacturer data ({} bytes, expected >= 22)",
            device.address(),
            data.len()
        );
        return None;
    }

    let product_type = ProductType::from_byte(data[0]);
    let serial_number = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
    let rssi = device.rssi().await.ok()?;

    Some(DiscoveredDevice {
        address: device.address(),
        product_type,
        serial_number,
        rssi,
        raw_manufacturer_data: data.clone(),
    })
}

/// Run the passive BLE advertising scanner.
/// Discovers Combustion devices and logs their information.
/// When a node is discovered, calls `on_node_found` with the device address.
///
/// This function runs forever (until the adapter stream ends or an error occurs).
pub async fn run_scanner(
    adapter: &Adapter,
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

                    log::info!(
                        "Combustion device: addr={} type={:?} serial={:08X} rssi={:?} data=[{}]",
                        addr,
                        discovered.product_type,
                        discovered.serial_number,
                        discovered.rssi,
                        discovered
                            .raw_manufacturer_data
                            .iter()
                            .map(|b| format!("{:02x}", b))
                            .collect::<Vec<_>>()
                            .join(" ")
                    );

                    if is_new && discovered.product_type.is_node() {
                        on_node_found(addr, &discovered);
                    }
                }
            }
            AdapterEvent::DeviceRemoved(addr) => {
                if let Some(product_type) = seen_devices.remove(&addr) {
                    log::debug!("Device removed: addr={} type={:?}", addr, product_type);
                }
            }
            _ => {}
        }
    }

    Ok(())
}
```

**Step 2: Update BLE module**

```rust
// sbc-service/src/ble/mod.rs
pub mod constants;
pub mod scanner;
```

**Step 3: Wire scanner into main.rs**

```rust
// sbc-service/src/main.rs
mod ble;
mod types;

use anyhow::Result;
use bluer::DiscoveryFilter;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init();
    log::info!("SBC service starting");

    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    log::info!("Using Bluetooth adapter: {}", adapter.name());
    adapter.set_powered(true).await?;

    // Configure for BLE (Low Energy) scanning
    adapter
        .set_discovery_filter(DiscoveryFilter {
            transport: bluer::DiscoveryTransport::Le,
            ..Default::default()
        })
        .await?;

    log::info!("Starting BLE scan for Combustion devices...");
    ble::scanner::run_scanner(&adapter, |addr, device| {
        log::info!(
            ">>> Node discovered: addr={} type={:?} serial={:08X}",
            addr,
            device.product_type,
            device.serial_number
        );
    })
    .await?;

    Ok(())
}
```

**Step 4: Build and verify**

Run: `cd sbc-service && cargo build`
Expected: Compiles successfully.

**Step 5: Test on hardware**

Run on the Raspberry Pi with Combustion devices nearby:

```bash
RUST_LOG=info cargo run
```

Expected output (device addresses and serials will vary):

```
[INFO] SBC service starting
[INFO] Using Bluetooth adapter: hci0
[INFO] Starting BLE scan for Combustion devices...
[INFO] Combustion device: addr=C2:71:0C:83:FE:50 type=PredictiveProbe serial=10005205 rssi=Some(-65) data=[01 05 52 00 10 ...]
[INFO] Combustion device: addr=C5:BC:E2:1B:48:F6 type=Booster serial=10005205 rssi=Some(-86) data=[05 05 52 00 10 ...]
[INFO] >>> Node discovered: addr=C5:BC:E2:1B:48:F6 type=Booster serial=10005205
```

Verify:

- Probes show as `PredictiveProbe`
- Nodes (Repeater/Display/Booster) show as their correct type
- `>>> Node discovered` messages appear only for node devices
- Raw manufacturer data bytes are 22 bytes long
- Serial numbers match what you see in the Combustion app

**Step 6: Commit**

```bash
git add sbc-service/src/ble/scanner.rs sbc-service/src/ble/mod.rs sbc-service/src/main.rs
git commit -m "feat: passive BLE advertising scanner for Combustion devices"
```

---

## Task 5: Node GATT Connection

**Files:**

- Create: `sbc-service/src/ble/connection.rs`
- Modify: `sbc-service/src/ble/mod.rs`
- Modify: `sbc-service/src/main.rs`

**Step 1: Create the connection module**

```rust
// sbc-service/src/ble/connection.rs
use anyhow::{anyhow, Context, Result};
use bluer::gatt::remote::Characteristic;
use bluer::{Adapter, Address, Device};
use futures::StreamExt;

use super::constants::{UART_SERVICE_UUID, UART_TX_UUID};

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
    let services = device.services().await.context("Failed to enumerate services")?;
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
    log::debug!("Found {} characteristics on UART service", characteristics.len());

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

/// Subscribe to UART TX notifications and log raw bytes as they arrive.
/// Returns when the notification stream ends (device disconnected).
pub async fn listen_uart_notifications(connection: &NodeConnection) -> Result<()> {
    log::info!("Subscribing to UART TX notifications...");
    let notify_stream = connection
        .tx_characteristic
        .notify()
        .await
        .context("Failed to subscribe to TX notifications")?;

    log::info!("Listening for UART TX notifications");
    futures::pin_mut!(notify_stream);

    while let Some(bytes) = notify_stream.next().await {
        let hex_str: String = bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");

        // Log the raw bytes. If we have at least the sync bytes + message type,
        // extract the message type for quick identification.
        if bytes.len() >= 5 && bytes[0] == 0xCA && bytes[1] == 0xFE {
            // Node response header: message type is at byte 4, with high bit set for responses
            let msg_type = bytes[4];
            let is_response = msg_type & 0x80 != 0;
            let base_type = msg_type & 0x7F;
            let type_label = match base_type {
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
            };
            log::info!(
                "UART TX: type=0x{:02X} ({}{}) len={} bytes=[{}]",
                msg_type,
                if is_response { "Response:" } else { "" },
                type_label,
                bytes.len(),
                hex_str
            );
        } else {
            log::info!("UART TX: len={} bytes=[{}]", bytes.len(), hex_str);
        }
    }

    log::warn!("UART TX notification stream ended (device disconnected?)");
    Ok(())
}
```

**Step 2: Update BLE module**

```rust
// sbc-service/src/ble/mod.rs
pub mod connection;
pub mod constants;
pub mod scanner;
```

**Step 3: Wire connection into main.rs (scanner-then-connect flow)**

Replace `main.rs` with an updated version that discovers a node, connects, and listens:

```rust
// sbc-service/src/main.rs
mod ble;
mod types;

use anyhow::Result;
use bluer::{Address, DiscoveryFilter};
use tokio::sync::mpsc;

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

    // Channel for the scanner to notify us when a node is found
    let (node_tx, mut node_rx) = mpsc::channel::<Address>(4);

    // Spawn the scanner in the background
    let scanner_adapter = adapter.clone();
    let scanner_handle = tokio::spawn(async move {
        if let Err(e) = ble::scanner::run_scanner(&scanner_adapter, move |addr, _device| {
            let _ = node_tx.try_send(addr);
        })
        .await
        {
            log::error!("Scanner error: {}", e);
        }
    });

    // Wait for the first node to be discovered
    log::info!("Scanning for MeatNet nodes...");
    let node_addr = node_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("Scanner ended before finding a node"))?;
    log::info!("Found node at {}, connecting...", node_addr);

    // Connect and listen
    let connection = ble::connection::connect_to_node(&adapter, node_addr).await?;
    ble::connection::listen_uart_notifications(&connection).await?;

    scanner_handle.abort();
    Ok(())
}
```

**Step 4: Build and verify**

Run: `cd sbc-service && cargo build`
Expected: Compiles successfully.

**Step 5: Test on hardware**

Run on the Raspberry Pi with a MeatNet node powered on and at least one probe active:

```bash
RUST_LOG=info cargo run
```

Expected output:

```
[INFO] SBC service starting
[INFO] Using Bluetooth adapter: hci0
[INFO] Scanning for MeatNet nodes...
[INFO] Combustion device: addr=C5:BC:E2:1B:48:F6 type=Booster serial=10005205 rssi=Some(-45) data=[...]
[INFO] >>> Node discovered: ...
[INFO] Found node at C5:BC:E2:1B:48:F6, connecting...
[INFO] Connecting to node at C5:BC:E2:1B:48:F6...
[INFO] Connected to node at C5:BC:E2:1B:48:F6
[INFO] Found UART service
[INFO] Found UART TX characteristic
[INFO] Subscribing to UART TX notifications...
[INFO] Listening for UART TX notifications
[INFO] UART TX: type=0x45 (ProbeStatus) len=103 bytes=[ca fe ...]
[INFO] UART TX: type=0x49 (Heartbeat) len=71 bytes=[ca fe ...]
[INFO] UART TX: type=0x45 (ProbeStatus) len=103 bytes=[ca fe ...]
```

Verify:

- Service discovery finds the UART service
- TX characteristic is found
- Notification subscription succeeds
- Raw bytes stream in continuously
- ProbeStatus (0x45) messages appear every few seconds per probe
- Heartbeat (0x49) messages appear periodically
- Message type labels are correct

**Step 6: Commit**

```bash
git add sbc-service/src/ble/connection.rs sbc-service/src/ble/mod.rs sbc-service/src/main.rs
git commit -m "feat: GATT connection to MeatNet node with UART TX notifications"
```

---

## Task 6: Reconnection Logic

**Files:**

- Modify: `sbc-service/src/ble/connection.rs`
- Modify: `sbc-service/src/main.rs`

**Step 1: Write tests for backoff calculation**

Add to `sbc-service/src/ble/connection.rs`:

```rust
/// Calculate the backoff duration for a given attempt number.
/// Uses exponential backoff: 1s, 2s, 4s, 8s, 16s, capped at 30s.
pub fn backoff_duration(attempt: u32) -> std::time::Duration {
    todo!()
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

**Step 2: Run tests to verify they fail**

Run: `cd sbc-service && cargo test`
Expected: FAIL — `not yet implemented` panic from `todo!()`

**Step 3: Implement backoff_duration**

```rust
pub fn backoff_duration(attempt: u32) -> std::time::Duration {
    let secs = 1u64.checked_shl(attempt).unwrap_or(30).min(30);
    std::time::Duration::from_secs(secs)
}
```

**Step 4: Run tests to verify they pass**

Run: `cd sbc-service && cargo test`
Expected: All tests pass (ProductType tests + backoff tests).

**Step 5: Add reconnection loop to main.rs**

Replace the connection section in main.rs with a reconnection loop:

```rust
// sbc-service/src/main.rs
mod ble;
mod types;

use anyhow::Result;
use bluer::{Address, DiscoveryFilter};
use tokio::sync::mpsc;

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

    // Channel for the scanner to notify us when a node is found
    let (node_tx, mut node_rx) = mpsc::channel::<Address>(4);

    // Spawn the scanner in the background
    let scanner_adapter = adapter.clone();
    tokio::spawn(async move {
        if let Err(e) = ble::scanner::run_scanner(&scanner_adapter, move |addr, _device| {
            let _ = node_tx.try_send(addr);
        })
        .await
        {
            log::error!("Scanner error: {}", e);
        }
    });

    // Wait for the first node to be discovered
    log::info!("Scanning for MeatNet nodes...");
    let node_addr = node_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("Scanner ended before finding a node"))?;
    log::info!("Found node at {}", node_addr);

    // Connection loop with reconnection
    let mut attempt: u32 = 0;
    loop {
        match ble::connection::connect_to_node(&adapter, node_addr).await {
            Ok(connection) => {
                attempt = 0; // Reset backoff on successful connection
                log::info!("Node connection established, listening for UART data...");

                if let Err(e) = ble::connection::listen_uart_notifications(&connection).await {
                    log::warn!("UART listener error: {}", e);
                }

                // If we get here, the notification stream ended (disconnect)
                log::warn!("Node disconnected");
            }
            Err(e) => {
                log::warn!("Connection to node failed: {}", e);
            }
        }

        let backoff = ble::connection::backoff_duration(attempt);
        log::info!("Reconnecting in {:?} (attempt {})...", backoff, attempt + 1);
        tokio::time::sleep(backoff).await;
        attempt += 1;
    }
}
```

**Step 6: Build and run tests**

Run: `cd sbc-service && cargo test && cargo build`
Expected: All tests pass, builds successfully.

**Step 7: Test on hardware**

Run on the Raspberry Pi:

```bash
RUST_LOG=info cargo run
```

Test reconnection by power-cycling the MeatNet node while the service is running.

Expected behavior:

1. Service connects and receives UART data normally
2. When node is powered off: `UART TX notification stream ended (device disconnected?)`
3. Backoff messages: `Reconnecting in 1s (attempt 1)...`, `Reconnecting in 2s (attempt 2)...`
4. When node is powered back on: `Connected to node`, data resumes
5. Backoff resets to 1s after successful reconnection

**Step 8: Commit**

```bash
git add sbc-service/src/ble/connection.rs sbc-service/src/main.rs
git commit -m "feat: automatic reconnection with exponential backoff"
```

---

## Task 7: Graceful Shutdown

**Files:**

- Modify: `sbc-service/src/main.rs`

**Step 1: Add Ctrl+C signal handling**

Update main.rs to handle graceful shutdown:

```rust
// sbc-service/src/main.rs
mod ble;
mod types;

use anyhow::Result;
use bluer::{Address, DiscoveryFilter};
use tokio::sync::mpsc;

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

    let (node_tx, mut node_rx) = mpsc::channel::<Address>(4);

    let scanner_adapter = adapter.clone();
    tokio::spawn(async move {
        if let Err(e) = ble::scanner::run_scanner(&scanner_adapter, move |addr, _device| {
            let _ = node_tx.try_send(addr);
        })
        .await
        {
            log::error!("Scanner error: {}", e);
        }
    });

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
                    result = ble::connection::listen_uart_notifications(&connection) => {
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

**Step 2: Build and verify**

Run: `cd sbc-service && cargo build`
Expected: Compiles successfully.

**Step 3: Test on hardware**

Run on the Raspberry Pi:

```bash
RUST_LOG=info cargo run
```

Test Ctrl+C at different stages:

1. During initial scanning (before a node is found) — should exit cleanly
2. While connected and receiving data — should disconnect from node and exit
3. During reconnection backoff — should exit immediately

Expected: Clean shutdown with no errors in all cases.

**Step 4: Commit**

```bash
git add sbc-service/src/main.rs
git commit -m "feat: graceful shutdown with Ctrl+C signal handling"
```

---

## Verification Checklist (End of Phase 1)

Run these checks to confirm Phase 1 is complete:

1. **Unit tests pass:**

   ```bash
   cd sbc-service && cargo test
   ```

   Expected: All tests pass (ProductType + backoff).

2. **Builds without warnings:**

   ```bash
   cd sbc-service && cargo build 2>&1 | grep -c warning
   ```

   Expected: 0 warnings (or only dependency warnings).

3. **Manual BLE test on Raspberry Pi:**

   ```bash
   RUST_LOG=info cargo run
   ```

   Verify:
   - [ ] Combustion probes are discovered with correct product type
   - [ ] MeatNet node is discovered and identified as a node
   - [ ] GATT connection to node succeeds
   - [ ] UART TX notifications stream in continuously
   - [ ] ProbeStatus (0x45) messages appear every few seconds
   - [ ] Heartbeat (0x49) messages appear periodically
   - [ ] Power-cycling the node triggers reconnection with backoff
   - [ ] Reconnection succeeds and data resumes after node powers back on
   - [ ] Ctrl+C exits cleanly at any point

4. **Code compiles on both dev machine and Raspberry Pi:**
   ```bash
   cargo check  # On dev machine (may not have BlueZ, but should type-check)
   ```
