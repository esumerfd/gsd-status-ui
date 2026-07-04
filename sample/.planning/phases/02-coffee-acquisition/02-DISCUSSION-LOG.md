# Discussion Log: Phase 2 — Coffee Acquisition

## Session 2026-07-01

**Q: Should the robot queue behind humans at the machine?**
A: Yes — yield radius 1 m, resume after the human leaves or 30 s of idle
machine.

**Q: What counts as a "spill" worth recovering?**
A: Any liquid outside the cup footprint larger than a bottle cap. Drips on
the drip tray are the machine's problem, not ours.

**Q: One retry or two?**
A: Two retries, then escalate. Three brews of waste is the budget ceiling.

## Session 2026-07-02

**Q: Paper cup crush incidents — grip or approach problem?**
A: Grip. Deformation starts at 4 N; classifier must run *before* the grasp,
not during. Locked into [[02-CONTEXT]] decision 1.

**Gray area flagged for research:** which vision model — resolved in
[[02-RESEARCH]] Q2 (YOLOv8-s).
