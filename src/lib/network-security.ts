// Settings for the "Network Security" card. Kill Switch and IPv6 actually take effect through the existing TUN
// settings (network-settings.ts); this stores the card's own protection mode, DNS leak protection, IPv6 policy tier,
// and malicious-domain blocking. Wiring up the behavior of protection mode / DNS leak / malicious domain (pushing it into the profile) is a follow-up.

export type ProtectionMode = "compat" | "balanced" | "strict";
export type Ipv6Policy = "follow" | "proxyOnly" | "block";

export type NetworkSecurity = {
  protectionMode: ProtectionMode;
  dnsLeakProtection: boolean;
  ipv6Policy: Ipv6Policy;
  maliciousDomainBlock: boolean;
};

const MODES: ProtectionMode[] = ["compat", "balanced", "strict"];
const IPV6: Ipv6Policy[] = ["follow", "proxyOnly", "block"];

export function defaultNetworkSecurity(): NetworkSecurity {
  return {
    protectionMode: "compat",
    dnsLeakProtection: true,
    ipv6Policy: "follow",
    maliciousDomainBlock: false,
  };
}

export function parseNetworkSecurity(value: unknown): NetworkSecurity | null {
  if (!value || typeof value !== "object") return null;
  const v = value as Record<string, unknown>;
  const base = defaultNetworkSecurity();
  return {
    protectionMode: MODES.includes(v.protectionMode as ProtectionMode)
      ? (v.protectionMode as ProtectionMode)
      : base.protectionMode,
    dnsLeakProtection: typeof v.dnsLeakProtection === "boolean" ? v.dnsLeakProtection : base.dnsLeakProtection,
    ipv6Policy: IPV6.includes(v.ipv6Policy as Ipv6Policy) ? (v.ipv6Policy as Ipv6Policy) : base.ipv6Policy,
    maliciousDomainBlock:
      typeof v.maliciousDomainBlock === "boolean" ? v.maliciousDomainBlock : base.maliciousDomainBlock,
  };
}
