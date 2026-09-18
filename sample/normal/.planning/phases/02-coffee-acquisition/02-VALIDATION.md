# Validation: Phase 2 — Coffee Acquisition

## Test Infrastructure

All plans validate through the simulator harness plus a physical smoke run.

```bash
make sim-test SUITE=coffee     # simulated validation, runs in CI
make smoke-run MACHINE=kitchen-2   # one physical end-to-end brew
```

## Per-Task Verification Map

| Task | Simulated check | Physical check |
|---|---|---|
| 02-01 machine detection | 500 rendered angles, recall ≥ 0.95 | 4 entry angles |
| 02-01 button press | force curve within envelope | brew starts 9/10 |
| 02-02 cup placement | ±5 mm in 1000 trials | 20 fills, 0 spills |
| 02-02 fill detection | state machine property tests | ±8% fill accuracy |
| 02-03 spill recovery | detection < 3 s on synthetic spills | sensor-dry < 60 s |

## Wave 0 Stubs

- `sim/tests/test_cup_state_machine.py` — written, failing (RED)
- `sim/tests/test_spill_detection.py` — written, failing (RED)
