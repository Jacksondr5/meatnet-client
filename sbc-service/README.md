# sbc-service

Minimal Phase 1 MeatNet BLE service scaffold for local validation.

This crate is currently aimed at MacBook development with a provisional `btleplug` backend behind a transport-neutral BLE boundary. It is intended to answer a narrow question first: can we discover, connect to, and inspect Combustion MeatNet devices from the host machine?

## What It Does

- Scans for Combustion BLE advertisements.
- Derives canonical MeatNet device keys from Combustion protocol data, not OS peripheral IDs.
- Connects to a selected device.
- Verifies required GATT services and characteristics.
- Reads Device Information values when available.
- Listens for notifications.
- Optionally writes a hex payload to the UART RX characteristic.

## Prerequisites

- Rust toolchain installed and `cargo` available in `PATH`.
- Bluetooth enabled on the host MacBook.
- At least one Combustion device powered on nearby.
- macOS Terminal or your terminal app must have Bluetooth permission if macOS prompts for it.

## Build

```bash
cd sbc-service
cargo build
```

## Commands

### Scan

Scans for Combustion advertisements and prints each discovered device.

```bash
cargo run -- scan --scan-seconds 6
```

Output includes:

- transport handle used only for the current session
- canonical key as `product-type:serial-number`
- advertisement family
- local name
- RSSI
- raw manufacturer payload in hex

### Inspect

Scans for one target device, connects, validates services, reads device info, and listens for notifications.

```bash
cargo run -- inspect --product-type display --serial T1000006XS
```

Optional flags:

- `--scan-seconds <n>`: how long to scan before selecting a target
- `--listen-seconds <n>`: how long to collect notifications after connect
- `--write-hex <hex>`: optional UART payload to write after service discovery

Example:

```bash
cargo run -- inspect \
  --product-type booster \
  --serial CR100010EB \
  --scan-seconds 6 \
  --listen-seconds 15
```

## Device Identity

Use the canonical Combustion identity, not the transport-layer peripheral ID.

- Probe-family devices use `product-type` plus the normalized 4-byte probe serial as uppercase 8-character hex.
- Node-family devices use `product-type` plus the 10-byte protocol serial string from node self-advertisements and Device Information.

Examples:

- `predictive-probe:00A1B2C3`
- `display:T1000006XS`
- `booster:CR100010EB`

## Current Scope

This is a validation tool, not the full production service.

- It is optimized for direct operator use from the terminal.
- The backend is currently `btleplug`.
- The transport boundary is already separated so the backend can be swapped later if Raspberry Pi validation requires it.

## Basic MacBook Workflow

1. Power on the Combustion device.
2. Run `cargo run -- scan --scan-seconds 6`.
3. Note the canonical key for the device you want.
4. Run `cargo run -- inspect --product-type <type> --serial <serial>`.
5. Review the validation summary, device information, and notifications.

## Notes

- Probe identities may appear through node-repeated advertisements even when the probe itself is not directly connectable. In that case `inspect` will refuse to connect unless it sees a direct probe advertisement.
- A displayed `handle=PeripheralId(...)` value is not a durable device identifier and should not be stored as canonical identity.
