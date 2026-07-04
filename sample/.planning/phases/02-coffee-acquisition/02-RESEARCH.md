# Research: Phase 2 — Coffee Acquisition

## Questions Investigated

### Q1: Why did the first button press crack the bezel?

The arm's default press force is 15 N — tuned for industrial panels. Coffee
machine bezels are ABS plastic; datasheet yield is ~6 N on a 4 mm button.
**Answer:** calibrate to 2.5 N with force feedback ramp.

### Q2: Which vision model detects the machine reliably?

| Model | Recall | Latency (CPU) | Notes |
|---|---|---|---|
| YOLOv8-n | 0.91 | 45 ms | misses matte-black machines |
| YOLOv8-s | 0.97 | 110 ms | chosen ✅ |
| Grounding DINO | 0.99 | 900 ms | too slow for the control loop |

### Q3: Can we trust the machine's own "ready" light?

No. Two of the three office machines have burnt-out ready LEDs. Heat
signature at the group head (>60 °C within 20 s) is the reliable signal.

## Pinned Choices

- Vision: YOLOv8-s at 640px, confidence 0.4
- Press: 2.5 N ramped over 300 ms
- Brew confirmation: thermal camera only
