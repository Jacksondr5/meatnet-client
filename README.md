# MeatNet Client

This repository contains the MeatNet client project, which will become a client application for interacting with Combustion MeatNet devices. The goal is to eventually provide BLE/companion functionality such as device discovery, telemetry streaming, and control workflows based on the protocol docs and implementation plans here.

## Repository contents

- `docs/`: project planning, design notes, and architecture documentation.
- `external-docs/`: external Combustion BLE specification documents used as protocol reference.
- `tools/`: focused validation spikes and developer utilities.

## Sync external documentation

`external-docs` is maintained as a separate repository. To fetch or update it, run:

```bash
./sync-docs.sh
```

This is the project’s docs sync command (sometimes referenced as `syncdocs`).

## Validation spike

A minimal Rust validation tool for cross-platform BLE checks with `btleplug` lives in:

```bash
tools/btleplug-spike/
```

Its purpose is to validate the MeatNet BLE requirements on macOS and Raspberry Pi before we commit to a production library choice.
