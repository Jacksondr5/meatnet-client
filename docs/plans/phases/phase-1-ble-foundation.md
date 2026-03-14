# Phase 1: BLE Foundation Implementation Plan

**Goal:** Establish BLE connectivity to Combustion Inc devices with a validation-first CLI — scan for advertising packets, connect to a MeatNet node via GATT, and receive raw UART bytes.

**Architecture:** The SBC service starts as a Rust async validation tool with a thin BLE transport boundary. Phase 1 should define that boundary first, then implement one provisional backend behind it while hardware validation is still in progress. The concrete BLE library choice is pending MeatNet-specific validation on MacBook and Raspberry Pi hardware. This phase targets explicit `scan` and `inspect` flows for advertisement discovery, GATT connection to one MeatNet node, and raw notification capture. Those flows should be expressed through transport-neutral types first. Raw bytes are logged to console for verification. This phase produces no parsed data and no long-running runtime behavior — just raw bytes flowing from hardware through BLE to the operator.

**Tech Stack:** Rust, one provisional BLE backend behind a transport-neutral interface, tokio 1.x, futures 0.3, anyhow, env_logger, `async-trait`, and `uuid`. Though, check to see if there are newer versions and use those instead.

**Implementation note:** Do not assume a `bluer`-first implementation. Start with a MeatNet validation spike using a cross-platform candidate if the goal remains one application-level codebase across MacBook and Raspberry Pi. If the assumptions in the BLE decision framework fail, stop and alert the user before continuing.

**Backend note:** The transport-neutral rules in this phase are normative. Any backend-specific code shown below is illustrative and should be adapted to the selected BLE backend after validation.

**Canonical identity rule:** Treat exact Combustion `productType + serialNumber` as the only durable device key. `serialNumber` is a normalized string derived from the correct advertisement family or confirmed from GATT Device Information data. BLE addresses and peripheral handles may be used to connect in the current process, but must never be used as persistent device identity.

**Reference docs:**

- `external-docs/probe_ble_specification.rst` — Probe advertising format, GATT services, UART messages
- `external-docs/meatnet_node_ble_specification.rst` — Node advertising format, GATT services, UART messages (node headers are different from probe headers)
- `docs/plans/2026-02-21-sbc-service-design.md` — SBC service internal architecture
- `docs/plans/2026-02-21-meatnet-companion-design.md` — System architecture overview
- `docs/plans/2026-03-07-ble-transport-abstraction-design.md` — MeatNet BLE decision logic, requirement classification, and stop conditions

**Key BLE facts from specs:**

- Combustion vendor ID: `0x09C7` (manufacturer data key in advertising)
- Direct probe advertisements carry probe identity using a 4-byte probe serial and include probe advertising data plus Thermometer Preferences
- Node repeated-probe advertisements carry probe identity using a 4-byte probe serial and include repeated probe advertising data
- Node self-advertisements for node-family devices such as Display and Booster carry a 10-byte node serial plus product-specific node payload fields
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
│   │   ├── transport.rs      # Transport-neutral traits and event types
│   │   ├── constants.rs      # BLE UUIDs, vendor ID, service constants
│   │   ├── scanner.rs        # Provisional backend scanner implementation
│   │   └── connection.rs     # Provisional backend node connection implementation
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
async-trait = "0.1"
env_logger = "0.11"
futures = "0.3"
log = "0.4"
tokio = { version = "1", features = ["full"] }
uuid = "1"
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

    /// Canonical slug used in persisted keys, routes, and fixture metadata.
    pub fn slug(&self) -> &'static str {
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

    #[test]
    fn slug_returns_canonical_product_type_names() {
        assert_eq!(ProductType::PredictiveProbe.slug(), "predictive-probe");
        assert_eq!(ProductType::MeatNetRepeater.slug(), "meatnet-repeater");
        assert_eq!(ProductType::GiantGrillGauge.slug(), "giant-grill-gauge");
        assert_eq!(ProductType::Display.slug(), "display");
        assert_eq!(ProductType::Booster.slug(), "booster");
        assert_eq!(ProductType::Unknown.slug(), "unknown");
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

    pub fn slug(&self) -> &'static str {
        match self {
            ProductType::Unknown => "unknown",
            ProductType::PredictiveProbe => "predictive-probe",
            ProductType::MeatNetRepeater => "meatnet-repeater",
            ProductType::GiantGrillGauge => "giant-grill-gauge",
            ProductType::Display => "display",
            ProductType::Booster => "booster",
        }
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

## Task 3: Transport Boundary

**Files:**

- Create: `sbc-service/src/ble/transport.rs`
- Modify: `sbc-service/src/ble/mod.rs`

**Step 1: Define the transport-neutral types and traits**

Create `sbc-service/src/ble/transport.rs`:

```rust
// sbc-service/src/ble/transport.rs
use async_trait::async_trait;
use uuid::Uuid;

use crate::types::ProductType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvertisementFamily {
    DirectProbe,
    NodeRepeatedProbe,
    NodeSelf,
}

#[derive(Debug, Clone)]
pub struct DiscoveryEvent {
    pub peripheral_handle: String,
    pub advertisement_family: AdvertisementFamily,
    pub product_type: ProductType,
    pub serial_number: String,
    pub rssi: Option<i16>,
    pub raw_manufacturer_data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct NotifyEvent {
    pub raw_bytes: Vec<u8>,
}

#[async_trait]
pub trait BleTransport {
    type Peripheral: ConnectedPeripheral;

    async fn start_scan(
        &self,
        on_discovery: impl Fn(DiscoveryEvent) + Send + 'static,
    ) -> anyhow::Result<()>;

    async fn connect(&self, peripheral_handle: &str) -> anyhow::Result<Self::Peripheral>;
}

#[async_trait]
pub trait ConnectedPeripheral {
    async fn discover_uart_characteristics(&self) -> anyhow::Result<UartCharacteristics>;
    async fn disconnect(&self) -> anyhow::Result<()>;
}

#[derive(Debug, Clone)]
pub struct UartCharacteristics {
    pub rx_uuid: Uuid,
    pub tx_uuid: Uuid,
}
```

The boundary should stay narrow. It exists to protect the rest of the application from backend-specific BLE APIs and identifiers.

**Step 2: Wire the module into `ble/mod.rs`**

```rust
// sbc-service/src/ble/mod.rs
pub mod transport;
```

**Step 3: Verify it compiles**

Run: `cd sbc-service && cargo build`
Expected: Compiles successfully.

**Step 4: Commit**

```bash
git add sbc-service/src/ble/transport.rs sbc-service/src/ble/mod.rs
git commit -m "feat: define transport-neutral BLE boundary"
```

---

## Task 4: BLE Constants

**Files:**

- Modify: `sbc-service/src/ble/mod.rs`
- Create: `sbc-service/src/ble/constants.rs`
- Modify: `sbc-service/src/main.rs`

**Step 1: Create the BLE module with constants**

```rust
// sbc-service/src/ble/mod.rs
pub mod transport;
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

## Task 5: Advertising Scanner

**Files:**

- Modify: `sbc-service/Cargo.toml`
- Create: `sbc-service/src/ble/scanner.rs`
- Modify: `sbc-service/src/ble/mod.rs`
- Modify: `sbc-service/src/main.rs`

This is the first hardware-dependent code. It cannot be unit tested — verification is done by running on the Raspberry Pi with Combustion devices nearby.

This task implements the first provisional backend behind the transport boundary. The example below shows a Linux backend implementation of the scanner contract. If validation selects a different backend, keep the same discovery model and identity rules but translate the backend-specific calls accordingly.

**Step 0: Add the provisional backend dependency**

Update `sbc-service/Cargo.toml` with the first backend selected for Phase 1. Example:

```toml
[dependencies]
bluer = { version = "0.17", features = ["full"] }
```

If a different provisional backend is chosen, add that library instead. The transport boundary remains the same either way.

**Step 1: Create the scanner module**

```rust
// sbc-service/src/ble/scanner.rs
use std::collections::HashMap;

use anyhow::Result;
use bluer::{Adapter, AdapterEvent, Address, Device};
use futures::{pin_mut, StreamExt};

use super::constants::COMBUSTION_VENDOR_ID;
use crate::types::ProductType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvertisementFamily {
    DirectProbe,
    NodeRepeatedProbe,
    NodeSelf,
}

/// Information about a discovered Combustion device.
/// `peripheral_handle` is transport-only; exact Combustion
/// `product_type + serial_number`
/// is the durable application identity.
#[derive(Debug)]
pub struct DiscoveredDevice {
    pub peripheral_handle: Address,
    pub advertisement_family: AdvertisementFamily,
    pub product_type: ProductType,
    pub serial_number: String,
    pub rssi: Option<i16>,
    pub raw_manufacturer_data: Vec<u8>,
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

fn parse_normalized_serial(
    advertisement_family: AdvertisementFamily,
    data: &[u8],
) -> Option<String> {
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
            let serial_bytes = &data[1..11];
            let serial = std::str::from_utf8(serial_bytes).ok()?;
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
/// Discovers Combustion devices and logs their information.
/// When a node is discovered, calls `on_node_found` with the current
/// transport handle. Callers must not treat that handle as durable identity.
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
                        "Combustion device: handle={} family={:?} key={}:{} rssi={:?} data=[{}]",
                        addr,
                        discovered.advertisement_family,
                        discovered.product_type.slug(),
                        discovered.serial_number,
                        discovered.rssi,
                        discovered
                            .raw_manufacturer_data
                            .iter()
                            .map(|b| format!("{:02x}", b))
                            .collect::<Vec<_>>()
                            .join(" ")
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

**Step 2: Update BLE module**

```rust
// sbc-service/src/ble/mod.rs
pub mod transport;
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
            ">>> Node discovered: handle={} key={}:{}",
            addr,
            device.product_type.slug(),
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

Expected output (transport handles and serials will vary):

```
[INFO] SBC service starting
[INFO] Using Bluetooth adapter: hci0
[INFO] Starting BLE scan for Combustion devices...
[INFO] Combustion device: handle=C2:71:0C:83:FE:50 family=DirectProbe key=predictive-probe:00F45A2C rssi=Some(-65) data=[01 2c 5a f4 00 ...]
[INFO] Combustion device: handle=C5:BC:E2:1B:48:F6 family=NodeSelf key=booster:CR100010EB rssi=Some(-86) data=[05 43 52 31 30 30 30 31 30 45 42 ...]
[INFO] >>> Node discovered: handle=C5:BC:E2:1B:48:F6 key=booster:CR100010EB
```

Verify:

- Canonical keys use product-type slugs such as `predictive-probe` and `booster`
- Nodes (Repeater/Display/Booster) show as their correct type
- `>>> Node discovered` messages appear only for node devices
- Direct probe, node repeated-probe, and node self-advertisements each produce the expected canonical key format
- Serial numbers match the device-family format from the Combustion specs and the node-family serial matches GATT Device Information after connect

**Step 6: Commit**

```bash
git add sbc-service/Cargo.toml sbc-service/src/ble/scanner.rs sbc-service/src/ble/mod.rs sbc-service/src/main.rs
git commit -m "feat: passive BLE advertising scanner for Combustion devices"
```

---

## Task 6: Node GATT Connection

**Files:**

- Create: `sbc-service/src/ble/connection.rs`
- Modify: `sbc-service/src/ble/mod.rs`
- Modify: `sbc-service/src/main.rs`

**Step 1: Create the connection module**

This task extends the provisional backend with node connection behavior behind the transport boundary. The example below shows a Linux backend implementation of the node connection workflow. If validation selects a different backend, preserve the same connect/discover/notify behavior and identity confirmation steps with that backend's API.

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
pub mod transport;
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
    log::info!("Found node via handle {}, connecting...", node_addr);

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
[INFO] Combustion device: handle=C5:BC:E2:1B:48:F6 family=NodeSelf key=booster:CR100010EB rssi=Some(-45) data=[...]
[INFO] >>> Node discovered: ...
[INFO] Found node via handle C5:BC:E2:1B:48:F6, connecting...
[INFO] Connecting to node via handle C5:BC:E2:1B:48:F6...
[INFO] Connected to node via handle C5:BC:E2:1B:48:F6
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
- ProbeStatus (0x45) messages appear periodically per probe
- Heartbeat (0x49) messages appear periodically
- Message type labels are correct

**Step 6: Commit**

```bash
git add sbc-service/src/ble/connection.rs sbc-service/src/ble/mod.rs sbc-service/src/main.rs
git commit -m "feat: GATT connection to MeatNet node with UART TX notifications"
```

---

## Task 7: Graceful Shutdown

**Files:**

- Modify: `sbc-service/src/main.rs`

**Step 1: Add Ctrl+C signal handling**

Update `main.rs` so the validation CLI exits cleanly on `Ctrl+C`:

```rust
// sbc-service/src/main.rs
const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(2);

async fn scan(scan_seconds: u64) -> Result<()> {
    let transport = BtleplugTransport::new_default().await?;
    println!("Scanning for {scan_seconds}s...");
    let discoveries =
        match await_with_shutdown_grace("scan", transport.scan(Duration::from_secs(scan_seconds)))
            .await?
        {
            OperationOutcome::Completed(discoveries) => discoveries,
            OperationOutcome::Interrupted(_) => {
                println!("Scan interrupted. Exiting cleanly.");
                return Ok(());
            }
        };

    // print discoveries...
    Ok(())
}

async fn inspect(...) -> Result<()> {
    // connect and inspect...

    let notifications = match await_with_shutdown_grace(
        "notification collection",
        peripheral.listen_notifications(Duration::from_secs(listen_seconds)),
    )
    .await?
    {
        OperationOutcome::Completed(notifications) => notifications,
        OperationOutcome::Interrupted(_) => {
            disconnect_with_report(peripheral.as_ref()).await;
            println!("Shutdown complete.");
            return Ok(());
        }
    };

    // print notifications...
    peripheral.disconnect().await?;
    Ok(())
}

async fn await_with_shutdown_grace<F, T>(label: &str, future: F) -> Result<OperationOutcome<T>>
where
    F: Future<Output = Result<T>>,
{
    tokio::pin!(future);

    tokio::select! {
        result = &mut future => result.map(OperationOutcome::Completed),
        _ = tokio::signal::ctrl_c() => {
            println!(
                "Shutdown requested during {label}; waiting up to {}s for the current operation to settle...",
                SHUTDOWN_GRACE_PERIOD.as_secs()
            );

            match tokio::time::timeout(SHUTDOWN_GRACE_PERIOD, &mut future).await {
                Ok(Ok(value)) => Ok(OperationOutcome::Interrupted(Some(value))),
                Ok(Err(_)) | Err(_) => Ok(OperationOutcome::Interrupted(None)),
            }
        }
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

  Expected: All tests pass.

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
   - [ ] ProbeStatus (0x45) messages appear periodically
   - [ ] Heartbeat (0x49) messages appear periodically
  - [ ] Ctrl+C exits cleanly at any point

4. **Code compiles on both dev machine and Raspberry Pi:**
   ```bash
   cargo check  # On dev machine (may not have BlueZ, but should type-check)
   ```
