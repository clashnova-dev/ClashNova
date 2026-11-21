#!/usr/bin/env python3
"""Regenerate src/lib/china-domains.ts (geosite:cn equivalent).

Source: github.com/felixonmars/dnsmasq-china-list accelerated-domains.china.conf.
Usage:
  python3 scripts/prepare-china-domains.py                 # download
  python3 scripts/prepare-china-domains.py accelerated.conf # use a local copy
"""
import pathlib
import re
import sys
import urllib.request

URL = "https://raw.githubusercontent.com/felixonmars/dnsmasq-china-list/master/accelerated-domains.china.conf"
root = pathlib.Path(__file__).resolve().parent.parent
line_re = re.compile(r"^server=/([a-z0-9.-]+)/")
dom_re = re.compile(r"^[a-z0-9-]+(?:\.[a-z0-9-]+)*$")

if len(sys.argv) > 1:
    text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
else:
    text = urllib.request.urlopen(URL, timeout=60).read().decode("utf-8")  # noqa: S310

seen: set[str] = set()
domains: list[str] = []
for line in text.splitlines():
    m = line_re.match(line.strip())
    if not m:
        continue
    d = m.group(1)
    if dom_re.match(d) and d not in seen:
        seen.add(d)
        domains.append(d)

if len(domains) < 10000:
    raise SystemExit(f"Refusing to write a suspiciously short list ({len(domains)} domains)")

out = root / "src/lib/china-domains.ts"
out.write_text(
    "// China mainland domains (geosite:cn equivalent, github.com/felixonmars/dnsmasq-china-list).\n"
    "// Injected as sing-box domain_suffix so CN sites stay direct in rule mode (both system-proxy\n"
    "// and TUN); the TUN DNS mirrors the same list so these names resolve to real domestic\n"
    "// addresses instead of fake-IP. rule_set/geosite is forbidden by the privileged service's\n"
    "// safe_config, so the list is inlined. Regenerate: python3 scripts/prepare-china-domains.py\n"
    f"// {len(domains)} domains.\n"
    f'const RAW =\n  "{",".join(domains)}";\n'
    'export const CHINA_DOMAINS: readonly string[] = RAW.split(",");\n',
    encoding="utf-8",
)
print(f"Wrote {out} with {len(domains)} domains")
