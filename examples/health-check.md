---
title: Server Health Check
runtime: bash
---

# Server Health Check

Quick checks to verify a server is healthy.

## Disk usage

```bash
df -h / | tail -1 | awk '{print "Disk: " $5 " used (" $3 " of " $2 ")"}'
```

## Memory

```bash
if command -v free > /dev/null 2>&1; then
    free -h | awk '/^Mem:/ {print "Memory: " $3 " used / " $2 " total"}'
else
    # macOS
    vm_stat | awk '/Pages active/ {printf "Active memory: %.0f MB\n", $3 * 4096 / 1048576}'
fi
```

## Load average

```bash
uptime | awk -F'load average:' '{print "Load:" $2}'
```

## Listening ports

```bash
if command -v ss > /dev/null 2>&1; then
    ss -tlnp 2>/dev/null | head -10
else
    # macOS
    lsof -iTCP -sTCP:LISTEN -P -n 2>/dev/null | head -10
fi
```
