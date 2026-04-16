---
title: Sysadmin Toolkit
requires:
  sandbox: python
  apt: [dnsutils, iputils-ping, net-tools, whois, traceroute]
  pip: [dnspython]
---

# Sysadmin Toolkit

A sandboxed sysadmin workbook with network tools that you probably don't have installed locally.

## DNS lookups

```bash
echo "=== dig ==="
dig +short example.com A
echo ""
echo "=== nslookup ==="
nslookup example.com | head -6
```

## Python DNS

```python
import dns.resolver

domain = "example.com"
for rtype in ["A", "AAAA", "MX", "NS"]:
    try:
        answers = dns.resolver.resolve(domain, rtype)
        for rdata in answers:
            print(f"  {rtype:5s} -> {rdata}")
    except dns.resolver.NoAnswer:
        print(f"  {rtype:5s} -> (no records)")
    except Exception as e:
        print(f"  {rtype:5s} -> error: {e}")
```

## Network info

```bash
echo "=== ifconfig ==="
ifconfig | head -10
echo ""
echo "=== netstat ==="
netstat -rn | head -5
```

## WHOIS

```bash
whois example.com | head -20
```
