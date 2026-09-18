---
status: resolved
trigger: "receipt printer times out after 30s on the first print of the day"
created: 2026-07-10T08:00:00Z
updated: 2026-07-10T08:45:00Z
---

## Resolution

root_cause: printer driver cold-start latency exceeds the 30s client timeout
fix: warm the printer connection on app launch instead of on first print
