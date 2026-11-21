#!/usr/bin/env python3
"""Regenerate src/lib/china-ip.ts with China mainland IPv4 + IPv6 ranges.

Usage:
  python3 scripts/prepare-china-ip.py                    # download both lists
  python3 scripts/prepare-china-ip.py ipv4.txt ipv6.txt  # use local copies
"""
import pathlib
import re
import sys
import urllib.request

V4_URL = "https://raw.githubusercontent.com/17mon/china_ip_list/master/china_ip_list.txt"
V6_URL = "https://raw.githubusercontent.com/gaoyifan/china-operator-ip/ip-lists/china6.txt"
root = pathlib.Path(__file__).resolve().parent.parent
v4_re = re.compile(r"^\d{1,3}(?:\.\d{1,3}){3}/\d{1,2}$")
v6_re = re.compile(r"^[0-9A-Fa-f:]+/\d{1,3}$")


def load(arg_index: int, url: str) -> str:
    if len(sys.argv) > arg_index:
        return pathlib.Path(sys.argv[arg_index]).read_text(encoding="utf-8")
    return urllib.request.urlopen(url, timeout=60).read().decode("utf-8")  # noqa: S310


def ranges(text: str, pattern: re.Pattern, minimum: int) -> list[str]:
    out = [line.strip() for line in text.splitlines() if pattern.match(line.strip())]
    if len(out) < minimum:
        raise SystemExit(f"Refusing to write a suspiciously short list ({len(out)} ranges)")
    return out


v4 = ranges(load(1, V4_URL), v4_re, 1000)
v6 = ranges(load(2, V6_URL), v6_re, 200)
out = root / "src/lib/china-ip.ts"
out.write_text(
    "// China mainland IP ranges, injected as sing-box ip_cidr route rules so traffic\n"
    "// to CN addresses stays direct in rule mode (GeoIP-CN bypass).\n"
    "// Regenerate: python3 scripts/prepare-china-ip.py\n"
    f"// IPv4 ({len(v4)}): APNIC-derived github.com/17mon/china_ip_list\n"
    f"// IPv6 ({len(v6)}): github.com/gaoyifan/china-operator-ip china6.txt\n"
    f'const V4 =\n  "{",".join(v4)}";\n'
    f'const V6 =\n  "{",".join(v6)}";\n'
    'export const CHINA_IPV4: readonly string[] = V4.split(",");\n'
    'export const CHINA_IPV6: readonly string[] = V6.split(",");\n',
    encoding="utf-8",
)
print(f"Wrote {out} with {len(v4)} IPv4 and {len(v6)} IPv6 ranges")
