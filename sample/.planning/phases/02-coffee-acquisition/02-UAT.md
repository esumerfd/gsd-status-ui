# UAT: Phase 2 — Coffee Acquisition

User acceptance criteria, walked with the human before the phase closes.

## Scenarios

### 1. Morning rush order

**Given** three people waiting in the kitchen
**When** the robot is asked for a black coffee
**Then** it queues politely, brews, and hands over a cup ≥ 90% full
**Status:** ⏳ pending

### 2. Paper cup, no spills

**Given** only paper cups on the shelf
**When** a brew completes
**Then** the cup is removed without deformation or drips
**Status:** ⏳ pending

### 3. Machine out of water

**Given** the reservoir is empty
**When** a brew is requested
**Then** the robot reports the problem instead of retrying blindly
**Status:** ⏳ pending

## Sign-off

- [ ] All scenarios pass on kitchen-2
- [ ] Incident log reviewed
