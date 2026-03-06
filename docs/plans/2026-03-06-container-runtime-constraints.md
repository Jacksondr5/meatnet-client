# Container Runtime Constraints Checklist

## Purpose

Define implementation-agnostic constraints for running the SBC service in a container, so Phase 6 can execute against clear operational acceptance criteria.

## Scope

- Runtime packaging and execution constraints only
- Applies to local development Linux hosts and Raspberry Pi deployment targets
- Does not prescribe a specific container engine or orchestrator

## Required Constraints

## 1. Image and Architecture

- Service MUST be packaged as an OCI-compatible image.
- Build pipeline MUST produce images for both amd64 and arm64.
- Release artifacts MUST include immutable version identifiers.

## 2. BLE and Host Access

- Container runtime MUST provide access needed for BlueZ/D-Bus BLE operations.
- Runtime MUST define required host integrations explicitly (device/socket/network capabilities).
- Missing BLE prerequisites MUST fail fast with clear startup errors.

## 3. Persistent State Mounts

- Durable spool/state MUST be persisted outside the container writable layer.
- Required persistent paths MUST be explicitly documented.
- Startup MUST verify mount availability and permissions before entering `live` state.

## 4. Configuration and Secrets

- Runtime configuration MUST be externalized (env vars and/or mounted config files).
- Machine credential material MUST not be baked into images.
- Invalid critical config MUST produce explicit `error` state and structured logs.

## 5. Lifecycle and Restart Semantics

- Runtime MUST support automatic start on host boot.
- Runtime MUST support automatic restart on process exit/failure.
- Runtime MUST apply bounded restart/backoff behavior.

## 6. Health and Observability

- Containerized service MUST expose liveness/readiness health signals.
- Runtime MUST provide structured logs accessible on host.
- Health state transitions (`starting`, `live`, `degraded`, `syncing`, `error`) MUST be emitted and externally observable.

## 7. Networking Behavior

- Runtime MUST tolerate delayed network availability on startup.
- Backend reconnect behavior MUST remain non-blocking and resilient.
- Debug endpoint policy MUST preserve loopback-default behavior unless explicit LAN override is configured.

## 8. Update and Rollback

- Update workflow MUST support deterministic image rollout by version.
- Rollback path MUST be defined and testable.
- Service version MUST be included in backend check-in metadata.

## 9. Resource Guardrails

- Runtime MUST define memory and disk usage guardrails.
- Storage-pressure behavior MUST preserve cook-critical data first.
- Exceeding guardrails MUST emit explicit `at_risk` operational signals.

## Phase 6 Acceptance Criteria

Phase 6 container runtime work is complete when all conditions are met:

1. Multi-arch images (amd64 + arm64) build successfully and can run.
2. BLE access works from the container on target SBC hardware.
3. Persistent spool survives container restart and host reboot.
4. Service auto-starts on power restore and auto-restarts on failure.
5. Health/readiness signals are available and mapped to runtime states.
6. Backend check-ins include device identity and service version.
7. Debug endpoint defaults to loopback-only unless LAN mode is explicitly enabled.
8. Rollback to previous image version is documented and validated.

## Related Docs

- [2026-03-06-operational-readiness-requirements.md](./2026-03-06-operational-readiness-requirements.md)
- [2026-03-06-reliability-contract-design.md](./2026-03-06-reliability-contract-design.md)
- [phases/README.md](./phases/README.md)
