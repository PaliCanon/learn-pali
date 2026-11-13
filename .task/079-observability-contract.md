# 079 Shared observability contract checks

status: proposed · rank: 4 · level: medium
request: pre-merge improve for structured logging (2026-11-13)

## Why

Learn and the API now emit the same cross-service resource fields using
different adapters. Search-front is the next consumer. Common conformance
examples would catch schema, release-identity and severity drift as these
services evolve, without requiring a live telemetry backend.

## Evidence

- Learn 2cfbaef: app/src/telemetry.rs and telemetry/server.rs attach the
  service identity to tracing events in both native and browser builds.
- API 0d3c901: api/src/telemetry.rs enriches serialized events centrally,
  including background-client events, and protects reserved metadata.
- Release tag and environment must remain distinct in both implementations.

## Proposal

Agree on common JSON fixtures and conformance cases for resource identity,
severity, tag/ref fallback, redaction and absent backends. Decide their
owner and whether a small shared crate is justified after comparing the
SSR, browser and host-owned CSR requirements. Keep service-specific events
in their own repositories.

## Cost and risk

Medium: multiple repositories and incompatible runtime feature sets. The
proposal changes no production code and authorizes no extraction, public
API change or telemetry export. It remains proposed after the parent merge.
