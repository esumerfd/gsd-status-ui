---
title: Remove debug logging from the brew loop
area: cleanup
---

# Remove debug logging from the brew loop

The brew loop printed a line per tick during bring-up. Resolved: dropped the
`eprintln!` calls once the state machine stabilized.
