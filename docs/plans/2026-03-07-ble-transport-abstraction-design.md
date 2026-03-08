# BLE Transport Decision Framework

## Overview

This document records the MeatNet-specific decision logic for BLE implementation strategy.

The question is not whether Linux BlueZ, macOS CoreBluetooth, or a cross-platform library is "best" in the abstract. The question is whether the MeatNet requirements can be handled safely through a cross-platform BLE abstraction while still letting us:

- develop on a MacBook,
- deploy on a Raspberry Pi,
- avoid premature architectural coupling to one OS-specific BLE library.

This document intentionally separates:

- MeatNet requirements that are safe to abstract away behind a library, from
- requirements where OS/backend differences may leak through and affect architecture.

## Critical Stop Condition

This is an architectural guardrail for all future agents working in this repository.

If any implementation or hardware validation work proves one of the assumptions in the "Decision Assumptions" section false, stop building immediately and alert the user that there may be an architectural problem.

Do not continue implementing around the issue silently. A failed assumption here means the backend choice, transport boundary, or deployment model may need to change.

## Current Recommendation

No final library choice should be treated as locked yet.

Current recommendation, subject to validation:

- Prefer a single application-level codebase if a cross-platform BLE library can satisfy the MeatNet requirements below.
- Treat Bluetooth MAC addresses as non-portable implementation details.
- Keep protocol parsing, session logic, fixture capture, and sync logic above any BLE library boundary.
- Validate real hardware behavior before committing to a production BLE backend.

At this stage, the likely candidates are:

- `Bleak` if optimizing for fastest cross-platform development,
- `btleplug` if optimizing for Rust and one cross-platform application codebase,
- `bluer` only if Linux-native behavior proves necessary for the final SBC runtime.

## Why This Boundary Exists

The Combustion BLE protocol itself appears to require standard BLE central behavior rather than low-level controller access. We need:

- advertisement discovery,
- manufacturer data parsing,
- GATT service and characteristic discovery,
- writes to the Nordic UART RX characteristic,
- notifications from the Nordic UART TX characteristic,
- optional direct probe status notifications.

Those needs can be expressed through a narrow transport interface.

## Requirements From The Combustion Specs

The MeatNet transport layer must support the following device behaviors:

- Discover advertisements carrying Combustion company ID `0x09C7`.
- Distinguish advertisement families using product type and payload format.
- Handle direct probe advertisements carrying a 4-byte probe serial number.
- Handle node repeated-probe advertisements carrying a 4-byte probe serial number.
- Handle device-specific node self-advertisements carrying a 10-byte node serial number.
- Connect to MeatNet nodes exposing UART service `6E400001-B5A3-F393-E0A9-E50E24DCCA9E`.
- Write outbound UART frames to RX characteristic `6E400002-B5A3-F393-E0A9-E50E24DCCA9E`.
- Subscribe to inbound UART frames on TX characteristic `6E400003-B5A3-F393-E0A9-E50E24DCCA9E`.
- Optionally subscribe directly to probe status characteristic `00000101-CAAB-3792-3D44-97AE51C1407A`.

The transport layer does not parse UART frames or manufacturer payloads beyond the minimal extraction needed for discovery and routing.

## MeatNet Requirement Classification

This table defines what we can safely treat as "abstracted by the library" and what we must still care about architecturally.

| MeatNet requirement | Safe to abstract behind BLE library? | Why | Validation needed |
| --- | --- | --- | --- |
| Scan for Combustion advertisements by company ID `0x09C7` | Yes | This is standard advertisement discovery plus manufacturer data access. | Confirm the chosen library exposes manufacturer data on both macOS and Linux. |
| Read probe and node manufacturer payload bytes | Yes | This is ordinary advertisement payload access. Parsing stays in our code. | Confirm payload bytes arrive unmodified and include all expected fields. |
| Connect to a node or probe over GATT | Yes | Standard BLE central behavior. | Confirm reliable connect and reconnect with real hardware. |
| Discover UART service and characteristics | Yes | Standard service and characteristic discovery. | Confirm the chosen library exposes the Nordic UART UUIDs correctly. |
| Write UART frames to RX characteristic | Yes | Standard characteristic write behavior. | Confirm payload size and write semantics are stable enough for Combustion commands. |
| Subscribe to UART TX notifications | Yes | Standard BLE notification subscription. | Confirm notification throughput is reliable during live cooks. |
| Subscribe to direct probe status notifications | Yes | Standard BLE notification subscription. | Confirm notification cadence works on both platforms. |
| Parse Combustion UART frames and advertising payloads | No, this is our responsibility | Library should only deliver bytes. Protocol correctness is application logic. | Test with fixtures and real hardware. |
| Device identity across reconnects | No, not fully | Platform BLE identifiers differ, and Combustion serial formats vary by advertisement family. | We must key devices by exact Combustion product type plus normalized serial string, and confirm node serials from GATT when connected. |
| Passive vs active scanning behavior | No, not fully | Scan semantics differ by OS/backend. | Validate whether active scanning on macOS is sufficient for capture needs. |
| Connection parameter tuning and low-level adapter behavior | No, not fully | Cross-platform libraries may not expose the same controls everywhere. | Validate real-world stability before committing production architecture. |
| Headless service behavior on Raspberry Pi | No, not fully | Deployment behavior depends on Linux runtime and OS integration. | Validate long-running Pi operation separately from MacBook development. |

## Decision Assumptions

The current architectural direction assumes all of the following are true:

1. A cross-platform BLE library can scan Combustion advertisements and expose manufacturer data on both macOS and Raspberry Pi Linux.
2. A cross-platform BLE library can connect to Combustion nodes and probes, discover the required GATT services, and perform write plus notify flows reliably enough for development and possibly production.
3. Combustion device identity can be modeled by exact Combustion product type plus normalized serial number, without depending on platform-specific BLE addresses.
4. macOS-specific scanning and identity differences do not block the development workflows we actually need.
5. Raspberry Pi production reliability does not require lower-level Linux-only BLE features that a cross-platform library cannot expose.

If any of these assumptions are disproven, stop implementation and escalate to the user immediately.

## Decision Logic

Use this decision logic when choosing the BLE library strategy:

1. If a cross-platform library satisfies all "Yes" rows in the requirement table and the "No, not fully" rows are manageable in our application design, prefer one application-level codebase across MacBook and Raspberry Pi.
2. If cross-platform support works for development but fails for long-running Raspberry Pi reliability, use the cross-platform library for development tooling only and switch production to a Linux-native backend.
3. If cross-platform support fails even for basic scan, connect, write, or notify behavior against real Combustion hardware, abandon the universal-library goal and standardize on Linux-native BLE for the core service.
4. Do not decide based on aesthetics or prior design-doc momentum. Decide based on validated MeatNet requirements.

## Candidate Strategies

### Strategy A: one cross-platform application codebase

Candidate libraries:

- `Bleak`
- `btleplug`

Use this strategy if:

- scan, connect, write, and notify all work reliably with Combustion devices on both macOS and Raspberry Pi,
- macOS scanning limitations do not block development and capture,
- Linux production behavior does not require Linux-only controls.

Benefits:

- one app-level BLE implementation,
- easiest MacBook development flow,
- smallest conceptual gap between development and deployment.

Risks:

- platform differences may still leak through around scanning semantics, permissions, and device identity,
- production behavior may be constrained by the lowest common denominator API.

### Strategy B: cross-platform development library plus Linux-native production backend

Candidate combination:

- `Bleak` or `btleplug` for development tooling,
- `bluer` for the SBC production runtime.

Use this strategy if:

- cross-platform BLE is good enough for capture and debugging,
- but Linux-native behavior is meaningfully better for long-running Raspberry Pi deployment.

Benefits:

- still supports MacBook-based development,
- allows production to use the strongest Linux-native option.

Risks:

- two BLE implementations,
- more maintenance and more chances for behavioral drift.

### Strategy C: Linux-native BLE everywhere that matters

Candidate library:

- `bluer`

Use this strategy if:

- cross-platform libraries fail against real Combustion hardware,
- or Pi production requirements require BlueZ-specific behavior that cannot be abstracted safely.

Benefits:

- strongest Linux control,
- simplest production story.

Risks:

- worst MacBook developer experience,
- development environment no longer matches the stated goal cleanly.

## Preferred Decision Order

The implementation order should be:

1. Validate a cross-platform library against real Combustion hardware on the MacBook.
2. Validate the same library on Raspberry Pi Linux.
3. Only fall back to a split-backend architecture if the Raspberry Pi validation reveals a real deficiency.

Do not choose a split-backend design before validating that a single cross-platform application-level solution actually fails.

## Layering

Recommended boundary:

```text
+--------------------------------------------------------------+
| Application                                                  |
| session manager | capture | debug server | Convex sync       |
+-----------------------------+--------------------------------+
| Protocol Core               | UART codec | advertising parse |
|                             | probe status parse             |
+-----------------------------+--------------------------------+
| BLE Transport Abstraction   | scan | connect | subscribe     |
+-----------------------------+--------------------------------+
| Backend Implementations     | bluer | btleplug               |
+--------------------------------------------------------------+
```

The protocol core should consume transport-neutral events and commands regardless of library choice.

## Transport Model

The abstraction should model three things:

1. discovery events,
2. connected peripheral operations,
3. backend capability reporting.

Illustrative Rust shape:

```rust
pub struct BleAdvertisement {
    pub peripheral_id: String,
    pub rssi: Option<i16>,
    pub local_name: Option<String>,
    pub manufacturer_data: Vec<ManufacturerRecord>,
    pub service_uuids: Vec<Uuid>,
    pub is_connectable: Option<bool>,
}

pub struct ManufacturerRecord {
    pub company_id: u16,
    pub data: Vec<u8>,
}

pub struct CombustionDiscovery {
    pub peripheral_id: String,
    pub product_type: ProductType,
    pub serial_number: String,
    pub source: DiscoverySource,
    pub rssi: Option<i16>,
    pub raw_manufacturer_data: Vec<u8>,
}

pub enum DiscoverySource {
    DirectProbeAdvertisement,
    NodeRepeatedProbeAdvertisement,
    NodeSelfAdvertisement,
}

pub struct BackendCapabilities {
    pub passive_scan_hint: bool,
    pub stable_adapter_identity: bool,
    pub stable_peripheral_address: bool,
}

#[async_trait]
pub trait BleTransport: Send + Sync {
    type Peripheral: ConnectedPeripheral;

    async fn capabilities(&self) -> BackendCapabilities;
    async fn start_scan(&self, filter: ScanFilter) -> Result<Pin<Box<dyn Stream<Item = BleAdvertisement> + Send>>>;
    async fn connect(&self, peripheral_id: &str) -> Result<Self::Peripheral>;
}

#[async_trait]
pub trait ConnectedPeripheral: Send + Sync {
    async fn discover_services(&self) -> Result<DiscoveredServices>;
    async fn subscribe(&self, characteristic: Uuid) -> Result<Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>>;
    async fn write(&self, characteristic: Uuid, data: &[u8], kind: WriteKind) -> Result<()>;
    async fn read(&self, characteristic: Uuid) -> Result<Vec<u8>>;
    async fn disconnect(&self) -> Result<()>;
}
```

This interface is intentionally narrow. We should add methods only when a concrete MeatNet requirement demands them.

## Identity Rules

The application should not depend on Bluetooth MAC addresses.

Reasons:

- macOS CoreBluetooth does not expose stable public device addresses in the same way Linux does.
- Combustion protocol data exposes stable identity, but the serial format depends on advertisement family and device type.
- Session and historical data should track the physical Combustion device, not the platform-specific BLE handle.

Recommended identifiers:

- `device_key = { product_type, serial_number }` for domain identity
- `peripheral_id` as an opaque backend handle used only by the transport layer

This is a hard rule:

- persist and query device state by `device_key`
- never persist `peripheral_id` as canonical identity
- never use BLE addresses or OS/library peripheral handles as durable identifiers in schemas, routes, or business logic

### Normalized serial rules

- `product_type` means the exact Combustion product type, not a broad `probe` or `node` bucket.
- `serial_number` is stored as a normalized string.
- For probes, normalize the 4-byte little-endian probe serial as uppercase 8-character hex.
- For node-family devices, normalize the 10-byte node serial as the protocol serial string exposed by the device-specific advertisement or the GATT Device Information service.
- When connected to a node-family device, read the GATT `Serial Number String` characteristic and use it to confirm or finalize the normalized serial.

### Advertisement identity families

- Direct probe advertisements use the probe advertising format and carry a 4-byte probe serial.
- Node repeated-probe advertisements reuse the probe identity format and carry a 4-byte probe serial for the repeated probe.
- Node self-advertisements are device-specific and carry a 10-byte node serial for products such as Display and Booster.

Identity parsing must follow the advertisement family, not just `company ID + product type`.

## Backend Responsibilities If We Need A Split

### `bluer` backend

Use for:

- Raspberry Pi and Linux SBC runtime,
- long-running service behavior,
- production deployment,
- Linux-specific operational integration with BlueZ and D-Bus.

Responsibilities:

- adapter selection and startup validation,
- scan stream from BlueZ discovery,
- connection establishment,
- characteristic discovery,
- notify subscription and write operations,
- clean mapping from BlueZ events into transport-neutral events.

### Cross-platform backend (`btleplug` or `Bleak`)

Use for:

- macOS development,
- Linux and macOS fixture capture,
- smoke testing BLE connectivity before SBC hardware is involved.

Responsibilities:

- provide the same transport-neutral discovery and connection events,
- hide CoreBluetooth and platform-specific peripheral identifiers,
- support connect, discover, read, write, and notify flows needed by Combustion devices.

It is acceptable if this backend offers weaker adapter control or different scanning semantics only if those limitations do not violate the MeatNet requirements above.

## Capability Differences We Must Design Around

### Passive vs active scanning

Linux with BlueZ may expose stronger scanning control than cross-platform stacks. The application should treat scan mode as a backend capability, not a protocol assumption.

Impact:

- capture and debug tooling can still run on macOS,
- production acceptance criteria should be validated on Linux,
- tests should assert parser behavior from captured payloads, not scan-mode specifics.

### Peripheral identifiers

macOS may return opaque CoreBluetooth identifiers instead of MAC addresses.

Impact:

- never persist peripheral IDs as device identity,
- route all device-level state through exact product type and normalized serial number,
- keep reconnect logic capable of rediscovering a device by advertisement contents.

### Connection parameter differences

The Combustion docs describe preferred connection intervals, but we should not assume every backend exposes those parameters or lets us tune them directly.

Impact:

- the transport abstraction should not include connection interval APIs,
- real-world throughput and notification stability must be validated with hardware,
- if Linux production needs lower-level tuning later, add it behind a backend-specific extension trait rather than polluting the common interface.

## Shared Core Responsibilities

The following code should remain backend-agnostic in every strategy:

- advertisement payload parsing,
- product type decoding,
- direct probe status parsing,
- UART framing and CRC,
- node request and response matching,
- reconnection policy state machine,
- fixture recording,
- debug event stream formatting,
- session management,
- Convex synchronization.

This is the main benefit of the abstraction. The only code that should care about the BLE library choice is the transport package and the runtime wiring that selects a backend.

## Proposed Repository Shape

```text
sbc-service/
  src/
    ble/
      transport/
        mod.rs
        types.rs
        bluer_backend.rs
        btleplug_backend.rs
      constants.rs
      discovery.rs
      node_connection.rs
      probe_connection.rs
    protocol/
    capture/
    debug/
```

Suggested separation:

- `ble/transport/*` contains backend-specific logic and transport-neutral types.
- `ble/discovery.rs` converts raw advertisements into `CombustionDiscovery`.
- `ble/node_connection.rs` handles node-specific UART workflows on top of `ConnectedPeripheral`.
- `protocol/*` remains pure parsing and framing code with no BLE dependencies.

## Backend Selection If Needed

Recommended startup model:

- default Linux SBC binary uses the chosen production backend,
- development capture CLI accepts an explicit backend choice,
- `auto` is acceptable only after the capability and behavior differences are well understood.

Do not attempt to hide every operational difference behind `auto`. Log the selected backend and its capability flags at startup.

## Implementation Plan

### Phase A: requirement validation spike

- Validate one cross-platform candidate against a MacBook with real Combustion hardware.
- Prove advertisement discovery, manufacturer data access, connect, write, and notify.
- Document any platform-specific behavior that leaks through.

### Phase B: Raspberry Pi validation

- Run the same validation on Raspberry Pi Linux.
- Determine whether the cross-platform candidate remains acceptable for production deployment.

### Phase C: architecture decision

- If validation succeeds, commit to the universal application-level solution.
- If validation fails only for production needs, split dev and production backends.
- If validation fails for basic Combustion workflows, standardize on a Linux-native stack.

## Testing Strategy

Tests should be split by layer:

- unit tests for advertisement parsing from raw manufacturer bytes,
- unit tests for UART framing and message parsing,
- backend contract tests for discovery, connect, write, and notify behavior,
- manual hardware validation for timing-sensitive behavior and long-running stability.

The capture fixture format should include:

- backend name,
- platform,
- peripheral handle,
- Combustion serial number,
- product type,
- raw advertisement or notification bytes,
- timestamp and RSSI when available.

## Recommendation

Do not lock the project to a split-backend architecture yet.

The correct next step is a MeatNet-specific validation spike using a cross-platform BLE library. If it satisfies the requirement table on both MacBook and Raspberry Pi, we should prefer one application-level codebase. If not, we should stop and revisit architecture instead of papering over the gap.
