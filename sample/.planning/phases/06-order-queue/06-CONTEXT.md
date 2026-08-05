# Context: Phase 6 — Order Queue

## Decisions Locked in Discussion

1. **One cup in flight at a time.** Batching is tempting but the tray only
   holds one cup safely, and a dropped tray costs more than a slow queue.
2. **First come, first served, no priorities.** Ranking colleagues is a social
   problem, not a robotics one.
3. **Orders expire after 20 minutes.** Nobody wants the coffee they forgot
   they asked for.

## Constraints

- The queue must survive a robot reboot; requesters should not have to re-ask.
- Cancellation has to work from the same surface the order came from.

## Out of Scope

- Scheduled or recurring orders
- Group orders for meetings

## Open Questions

- Where does the queue live — on the robot, or in the office server?
- What does the robot say when the queue is longer than the coffee supply?
