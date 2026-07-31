# REQUIREMENTS: Robot Coffee Service

**Milestone:** v0.1 — Coffee Delivery MVP

What v0.1 must do, phrased so each line can be verified by a person watching the
robot rather than by reading its logs.

## User Stories

- **US-1** — As someone at a desk, I ask for a coffee and get one, so I don't
  have to leave the desk.
- **US-2** — As someone in the kitchen, I can walk past the robot without it
  stopping dead or spilling, so it doesn't make the kitchen worse.
- **US-3** — As the person who owns the robot, I can see where it thinks it is,
  so I can tell a lost robot from a slow one.

## Functional Requirements

| ID | Requirement | Phase |
|---|---|---|
| FR-1 | Produce an occupancy map of the current floor. | 1 |
| FR-2 | Drive to the kitchen on command, arriving within 2 minutes. | 1 |
| FR-3 | Report current position on demand. | 1 |
| FR-4 | Recognise a clean cup on the rack and pick it up. | 2 |
| FR-5 | Operate the brewer for a single cup of drip coffee. | 2 |
| FR-6 | Carry a full cup without spilling more than 5 ml. | 2 |
| FR-7 | Deliver to the requester's desk and announce arrival. | 3 |
| FR-8 | Yield right of way to any human in a corridor. | 3 |

## Non-Functional Requirements

- **NFR-1** — A delivery completes in under 5 minutes end to end.
- **NFR-2** — The robot never applies force above 10 N to anything it touches.
- **NFR-3** — Battery lasts a full working day of 20 deliveries.

## Out of Scope

- Espresso, milk steaming, and anything involving a portafilter.
- Multi-floor delivery (no lifts in v0.1).
- Taking payment.

## Acceptance Criteria

1. Twenty consecutive requests each end with a full cup on the right desk.
2. No collision with a person or a wall across those twenty runs.
3. Every run is traceable to a position log the owner can read.

## Definition of Done

All three phases verified, the twenty-run trial recorded, and the kitchen team
signs off that the robot is not in the way.
