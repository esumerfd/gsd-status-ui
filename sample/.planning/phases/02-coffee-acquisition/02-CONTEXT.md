# Context: Phase 2 — Coffee Acquisition

## Decisions Locked in Discussion

1. **Paper cups first.** Ceramic and glass support can slip to Phase 3 if
   grip classification runs late.
2. **No milk steaming in v0.1.** Black coffee only; the frother is a
   different machine with a different interface.
3. **Tray scale is the source of truth for fill level** — cheaper and more
   robust than vision through steam.

## Constraints

- The kitchen is shared; the robot yields to humans within 1 m and pauses
  all arm motion.
- Brew cycle must finish within the 4-minute office coffee-rush window.

## Out of Scope

- Espresso drinks, tea, hot chocolate
- Restocking beans or water
