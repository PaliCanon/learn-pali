# 081 Recover missing-persona handoffs and synchronize task status

status: proposed
rank: 3
level: small
type: process
request: proposed by retro 080 on 2026-11-13; not accepted by parent closure

## Why and evidence

Learn 080's grounding voice returned unknown-persona (28g375e66sos), while
process b20ca9408781 and design/review placeholders did not reflect the later
authorized implementation and merge. Process-only retrospective e6b1817a3622
and Gemini review v4g6b8lt22cu identify this recovery/reporting gap.

## Proposal

Inspect the shared task/handoff workflow and published persona catalogue.
Define an explicit, recorded recovery path when a persona is unavailable;
synchronize task and Studio stage/evidence at each boundary. Preserve human
design/merge/release gates, do not invent a persona response, and never treat
a fallback as authorization to export source to another provider.

## Scope and cost

Shared workflow/catalogue follow-up, not an origin-guard change. Confirm the
owning repository and rights before any implementation. No seed publication,
new right, workflow modification or deployment is authorized by this proposal.
Production release of Learn 080 remains separately pending.
