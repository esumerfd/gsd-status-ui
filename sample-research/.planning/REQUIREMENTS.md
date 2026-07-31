# REQUIREMENTS: Robot Tea Service

**Milestone:** v0.1 — One Good Cup

## User Stories

- **US-1** — As a tea drinker, I get a cup brewed for the time my leaf actually
  wants, so it isn't stewed.
- **US-2** — As a tea drinker, I can say "stronger next time" and have that
  remembered.

## Functional Requirements

| ID | Requirement |
|---|---|
| FR-1 | Heat water to a set temperature within 2 °C. |
| FR-2 | Steep for a per-leaf duration, timed from immersion. |
| FR-3 | Remove the leaf at the end of the steep without being asked. |
| FR-4 | Remember a per-person strength preference across cups. |

## Non-Functional Requirements

- **NFR-1** — Water reaches temperature in under 90 seconds.
- **NFR-2** — The brewer is safe to leave running unattended.

## Out of Scope

- Milk, sugar, and the argument about which goes in first.
- Loose-leaf blending.

## Acceptance Criteria

1. Ten cups in a row land within 2 °C and 5 seconds of their targets.
2. A stated preference survives a restart.

## Definition of Done

Requirements above are verified by a taste panel of three, and the temperature
log for all ten cups is kept.
