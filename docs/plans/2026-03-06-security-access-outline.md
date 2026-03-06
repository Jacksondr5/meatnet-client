# Security and Access Model Outline

## Purpose

Capture the current MVP security/access decisions so they are explicit, while deferring detailed implementation design until the dedicated auth/provisioning phase.

## Decisions Locked In (MVP)

- Deployment model: single-user household.
- Data model uses explicit ownership boundaries (`ownerId`) for device/session/command-scoped data.
- Web app auth uses Clerk integrated with Convex user auth.
- SBC auth uses machine authentication via an M2M identity, not user OAuth runtime delegation.
- SBC machine credential is long-lived and provisioned per deployment.
- SBC credential handling is minimal and pragmatic:
  - stored locally with strict file permissions
  - validated at startup
  - rotated/revoked on explicit reprovision or suspected leak
- Debug server security posture is intentionally lightweight for home-network use:
  - loopback-only by default
  - LAN exposure is explicit opt-in
  - LAN mode requires shared debug token
  - debug endpoints are read-only by default
  - debug server can be fully disabled

## Why This Is The Chosen Level

- Avoids over-securing a home-network-first MVP.
- Preserves critical ownership boundaries and machine/user separation.
- Keeps implementation complexity bounded while still leaving room for future tightening.

## Deferred for Detailed Design (Phase 5)

- Exact Convex auth rule definitions per table/query/mutation.
- Exact SBC machine authentication provisioning flow for first M2M credential bootstrap.
- Concrete credential rotation/revocation runbook and UI/CLI surfaces.
- Failure-mode UX for auth errors (expired/revoked/misconfigured credentials).

## Terminology Notes

- `Clerk`: identity provider for user authentication in the web app.
- `M2M`: machine-to-machine credential model used by the SBC runtime.
- `Machine authentication`: non-user auth path for unattended service behavior on the SBC.

## Clerk Documentation References

- Clerk + Convex integration guide:
  - https://clerk.com/docs/guides/development/integrations/databases/convex
- Clerk machine authentication overview:
  - https://clerk.com/docs/guides/development/machine-auth/overview
- Clerk M2M tokens guide:
  - https://clerk.com/docs/guides/development/machine-auth/m2m-tokens

Reference check date: 2026-03-06.

## Phase Link

Detailed design and implementation belongs to:

- [docs/plans/phases/README.md](./phases/README.md) — Phase 5: Auth + Provisioning Foundation

## Related Docs

- [2026-02-21-meatnet-companion-design.md](./2026-02-21-meatnet-companion-design.md)
- [2026-02-21-convex-schema-design.md](./2026-02-21-convex-schema-design.md)
- [2026-02-21-sbc-service-design.md](./2026-02-21-sbc-service-design.md)
- [2026-03-06-reliability-contract-design.md](./2026-03-06-reliability-contract-design.md)
