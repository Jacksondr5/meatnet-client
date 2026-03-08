# btleplug MeatNet Validation Spike

This tool is a narrow validation spike for the MeatNet BLE requirements.

It is not production code. Its job is to answer:

- can `btleplug` discover Combustion advertisements on this machine?
- can it identify devices by `productType + serialNumber`?
- can it connect, discover services, and subscribe to notifications?

## Commands

Scan for nearby Combustion devices:

```bash
cargo run -- scan --scan-seconds 6
```

Inspect a specific node or probe by canonical device key:

```bash
cargo run -- inspect --product-type booster --serial 10005205
```

Inspect and listen for notifications for 15 seconds:

```bash
cargo run -- inspect \
  --product-type predictive-probe \
  --serial 10005205 \
  --listen-seconds 15
```

Optionally write a manual test payload to the UART RX characteristic:

```bash
cargo run -- inspect \
  --product-type booster \
  --serial 10005205 \
  --write-hex cafe0000
```

`--write-hex` is intentionally manual. Only use it with a payload you trust.

## macOS note

On macOS Big Sur or later, the terminal application running this tool must have Bluetooth permission. `btleplug` documents this requirement in its README.
