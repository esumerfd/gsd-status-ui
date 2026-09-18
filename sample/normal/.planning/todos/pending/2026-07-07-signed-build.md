---
created: 2026-07-07T15:30:43.056Z
title: Official signed build process for pr-monitor apps
area: tooling
files:
  - apps/pr-monitor/Makefile
  - apps/pr-status-menu/Makefile
---

## Problem

Ad-hoc signed binaries get a fresh cdhash every build, invalidating the
keychain ACL that "Always Allow" created.
