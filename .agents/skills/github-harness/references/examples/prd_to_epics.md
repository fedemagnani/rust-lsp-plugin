# PRD To Epics Example

## Goal

This example shows how a product requirement document becomes a small set of clear, self-contained
epic issues.

## Decomposition Rule

- Epic titles **MUST** stay short and concept-level.
- Epics **MUST** form a clear partition of the product-requirement space: each epic owns one
  independent sub-domain in terms of concepts, responsibilities, and governing principles.
- Epics **MUST NOT** be linked to one another through parent-child or dependency relationships.
- Epic bodies **MUST** be self-contained and **MUST** extract the context needed to understand the
  epic without reopening the product requirement document.
- Epic bodies **MAY** be longer than ordinary implementation issues when the extra context is
  needed for
  precise feature-complete requirements.
- When the epic states requirements, it **MUST** use bold RFC 2119 / RFC 8174 keywords such as
  **MUST**, **MUST NOT**, and **SHOULD**.
- Epic titles **MUST NOT** include implementation-detail wording.
- Ordinary implementation issues **SHOULD** stay shorter and narrower than the epic that owns them.

## Example Epic Set

For a telemedicine product, a valid decomposition is:

- `epic: clinician directory`
- `epic: appointment scheduling`
- `epic: consultation session`
- `epic: billing and claims`

Each epic owns a different sub-domain and they define a partition of the whole product requirements
set. Directory defines who can be booked, scheduling governs time-slot integrity, consultation
session owns the live visit experience, and billing and claims owns money movement and
reimbursement.

## Example Epic Draft

Title:

```text
epic: appointment scheduling
```

Body:

```markdown
## Description
Current state: patients can discover clinicians, but they cannot reserve, reschedule, or cancel an
appointment. The product has no scheduling domain that turns clinician working hours, visit
durations, notice windows, and timezone rules into safe bookable slots. Without this domain, the
platform cannot guarantee whether an appointment request is valid, confirmed, or conflicting with
an existing reservation.

This epic is limited to scheduling. It does not own clinician discovery, the live consultation
experience, or billing after the visit.

## Contract
The scheduling domain **MUST** own slot generation, slot reservation, rescheduling, cancellation,
and conflict prevention for bookable visits.
The scheduling domain **MUST** preserve slot integrity across concurrent requests.
The scheduling domain **MUST** expose a stable appointment identifier that downstream consultation
and billing domains can consume.
The scheduling domain **MUST NOT** depend on the consultation session domain to decide whether a
slot is bookable.

## Acceptance
1. Patients **MUST** be able to see bookable slots in their local timezone.
2. The platform **MUST** reject double-booking, even under concurrent slot claims.
3. Rescheduling and cancellation **MUST** enforce notice-window rules.
4. Confirmed appointments **MUST** be handed off to consultation and billing through a stable
   appointment record.

## Design notes
This epic is intentionally self-contained. A qualified engineer **SHALL** understand the scheduling
problem, its boundaries against adjacent domains, and the feature-complete target without reopening
the source product requirement document.
```

## Example Implementation Issue Under The Epic

```text
feat: prevent double-booking during concurrent slot claims
```

The implementation issue stays smaller. It keeps only the local implementation context needed for
one delivery unit, while the epic carries the broader product context and feature-complete
definition. The parent-child link in GitHub is sufficient; the epic body **MUST NOT** mirror the
list of child issues in the issue body.

When a same-purpose harness script exists, the agent **MUST** use it and **MUST NOT** substitute
raw `gh` or other GitHub CLI commands unless the human explicitly asks to use `gh` or the GitHub
CLI.

See `references/PLANNING_WORKFLOWS.md` for the one-time planning flow.
