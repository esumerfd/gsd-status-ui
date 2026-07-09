# ROADMAP: Robot Coffee Service

**Milestone:** v0.1 — Coffee Delivery MVP
**Status:** executing (Phase 2 of 3)

A three-phase build toward a robot that can navigate the office, acquire a fresh
cup of coffee, and deliver it to the requester without incident. Each phase is an
end-to-end, demoable capability rather than a horizontal technical layer.

## Phases

- [x] **Phase 1: Navigation Skeleton**
- [ ] **Phase 2: Coffee Acquisition**
- [ ] **Phase 3: Delivery Etiquette**

## Phase Details

### Phase 1: Navigation Skeleton
**Goal:** The robot can build a map of the office and drive to the kitchen on command.
**Success Criteria**:
1. Robot produces an occupancy map of the current floor.
2. Given "go to kitchen", the robot arrives within 2 minutes with no collisions.
3. Robot reports its current position on demand.

### Phase 2: Coffee Acquisition
**Goal:** At the machine, the robot brews a cup and confirms it is filled correctly.
**Success Criteria**:
1. Robot locates and operates the coffee machine unaided.
2. Robot seats a cup and detects fill level to within 5%.
3. On a spill or mis-fill, the robot recovers and retries without human help.

### Phase 3: Delivery Etiquette
**Goal:** The robot delivers the cup to the requester politely and safely.
**Success Criteria**:
1. Robot carries a full cup to the requester without spilling.
2. Robot announces arrival and waits for the cup to be taken before releasing.
3. Robot yields right-of-way to people in hallways.

## Plan Index

### Phase 1: Navigation Skeleton
- [x] 01-01-PLAN.md — map the office and drive to the kitchen

### Phase 2: Coffee Acquisition
- [x] 02-01-PLAN.md — locate and operate the coffee machine
- [ ] 02-02-PLAN.md — cup handling and fill-level detection
- [ ] 02-03-PLAN.md — spill recovery and retry loop

### Phase 3: Delivery Etiquette
_(plans not yet decomposed)_
