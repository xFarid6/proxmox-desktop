#!/usr/bin/env python3
"""Scan subnet for occupied IPs. Default: 192.168.1.0/24"""

import platform
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from ipaddress import ip_network

def ping_ip(ip):
    """Ping single IP. Return (ip, True) if occupied, (ip, False) if free."""
    try:
        if platform.system() == "Windows":
            cmd = ["ping", "-n", "1", "-w", "1000", str(ip)]
        else:
            cmd = ["ping", "-c", "1", "-W", "2", str(ip)]

        result = subprocess.run(
            cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=5
        )
        return (str(ip), result.returncode == 0)
    except (subprocess.TimeoutExpired, Exception):
        return (str(ip), False)

def scan_subnet(subnet_str, max_workers=8):
    """Scan subnet for occupied IPs."""
    try:
        subnet = ip_network(subnet_str, strict=False)
    except ValueError as e:
        print(f"Invalid subnet: {e}", file=sys.stderr)
        sys.exit(1)

    occupied = []
    free = []

    print(f"Scanning {subnet}...", file=sys.stderr)

    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        futures = {executor.submit(ping_ip, ip): ip for ip in subnet.hosts()}

        for future in as_completed(futures):
            ip, is_occupied = future.result()
            if is_occupied:
                occupied.append(ip)
            else:
                free.append(ip)

    occupied.sort(key=lambda x: tuple(map(int, x.split('.'))))
    free.sort(key=lambda x: tuple(map(int, x.split('.'))))

    print(f"\nOccupied ({len(occupied)}):", file=sys.stderr)
    for ip in occupied:
        print(f"  {ip}")

    print(f"\nFree ({len(free)}):", file=sys.stderr)
    if len(free) <= 10:
        for ip in free:
            print(f"  {ip}")
    else:
        print(f"  First 5: {', '.join(free[:5])}")
        print(f"  Last 5: {', '.join(free[-5:])}")
        print(f"  (and {len(free) - 10} more...)")

    # Suggest first free
    if free:
        print(f"\nFirst free IP: {free[0]}", file=sys.stderr)

if __name__ == "__main__":
    subnet = sys.argv[1] if len(sys.argv) > 1 else "192.168.1.0/24"
    scan_subnet(subnet)
