# ROADMAP: Robot Coffee Service

**Milestone:** v0.1 — Coffee Delivery MVP
**Status:** executing (Phase 2 of 9)

A build toward a robot that can navigate the office, acquire a fresh cup of
coffee, and deliver it to the requester without incident. Each phase is an
end-to-end, demoable capability rather than a horizontal technical layer.

Phases 1-8 deliberately sit at eight different maturities — one per stage the
status panel can paint, so every stage colour is on screen at once. Phases 4-7
were worked out of order while Phase 3 waited on a hardware part. Phase 9
doubles up on the planned stage on purpose: it exists to exercise the status
viewer's structural-tag heading conversion, not to add a ninth maturity — there
are only eight stages the panel can paint.

## Phases

- [x] **Phase 1: Navigation Skeleton**
- [ ] **Phase 2: Coffee Acquisition**
- [ ] **Phase 3: Delivery Etiquette**
- [ ] **Phase 4: Milk Steaming**
- [ ] **Phase 5: Cup Inventory**
- [ ] **Phase 6: Order Queue**
- [ ] **Phase 7: Multi-Floor Delivery**
- [~] **Phase 8: Voice Ordering**
- [ ] **Phase 9: XML Rendering**

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

### Phase 4: Milk Steaming
**Goal:** The robot steams milk and pours a drinkable latte.
**Success Criteria**:
1. Milk lands between 60 °C and 64 °C, twenty jugs running.
2. Milk-to-coffee ratio within 10% of 1:3.
3. Nothing spills outside the drip tray.

### Phase 5: Cup Inventory
**Goal:** The robot knows how many clean cups are left and asks for a restock.
**Success Criteria**:
1. Cup count within ±1 of a hand tally.
2. A restock request fires before the shelf runs dry.
3. An unreadable shelf reads as unknown, never as zero.

### Phase 6: Order Queue
**Goal:** The robot takes orders from several people and serves them in turn.
**Success Criteria**:
1. Orders survive a robot reboot.
2. Requesters can cancel from wherever they ordered.
3. Stale orders expire instead of piling up.

### Phase 7: Multi-Floor Delivery
**Goal:** The robot rides the elevator to deliver on floors 2 and 3.
**Success Criteria**:
1. Robot calls and boards the elevator unaided.
2. Cup stays level through acceleration.
3. Robot recovers when the car arrives full.

### Phase 8: Voice Ordering
**Goal:** _(abandoned)_ Take spoken orders in the kitchen.
**Success Criteria**:
1. ~~Recognize an order over the grinder.~~

Dropped: the kitchen is far too loud for the mic array, and the chat ordering
surface from Phase 6 covers the same need for a fraction of the work.

### Phase 9: XML Rendering
**Goal:** The status viewer renders this project's own bare structural tags as a nested heading outline.
**Success Criteria**:
1. Heading level tracks nesting depth and caps at six.
2. A tag's attributes never reach the rendered heading text.
3. Tags inside a fenced code block stay literal instead of becoming headings.

## Plan Index

### Phase 1: Navigation Skeleton
- [x] 01-01-PLAN.md — map the office and drive to the kitchen

### Phase 2: Coffee Acquisition
- [x] 02-01-PLAN.md — locate and operate the coffee machine
- [ ] 02-02-PLAN.md — cup handling and fill-level detection
- [ ] 02-03-PLAN.md — spill recovery and retry loop

### Phase 3: Delivery Etiquette
_(plans not yet decomposed)_

### Phase 4: Milk Steaming
- [x] 04-01-PLAN.md — steam milk to temperature
- [x] 04-02-PLAN.md — pour a drinkable latte

### Phase 5: Cup Inventory
- [ ] 05-01-PLAN.md — count cups on the shelf
- [ ] 05-02-PLAN.md — ask for a restock before running dry

### Phase 6: Order Queue
_(discussed; plans not yet decomposed)_

### Phase 7: Multi-Floor Delivery
_(discussion still open)_

### Phase 8: Voice Ordering
_(abandoned before planning)_

### Phase 9: XML Rendering
- [ ] 09-01-PLAN.md — structural tags as a nested heading outline
