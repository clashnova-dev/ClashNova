// 「网络安全」卡片的设置。Kill Switch 与 IPv6 的实际生效落在既有 TUN 设置
// (network-settings.ts)；这里保存卡片自身的防护模式、DNS 泄漏防护、IPv6 策略档位
// 与恶意域名拦截。防护模式 / DNS 泄漏 / 恶意域名的行为接线(profile 下发)为后续项。

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
  return { protectionMode: "compat", dnsLeakProtection: true, ipv6Policy: "follow", maliciousDomainBlock: false };
}

export function parseNetworkSecurity(value: unknown): NetworkSecurity | null {
  if (!value || typeof value !== "object") return null;
  const v = value as Record<string, unknown>;
  const base = defaultNetworkSecurity();
  return {
    protectionMode: MODES.includes(v.protectionMode as ProtectionMode) ? (v.protectionMode as ProtectionMode) : base.protectionMode,
    dnsLeakProtection: typeof v.dnsLeakProtection === "boolean" ? v.dnsLeakProtection : base.dnsLeakProtection,
    ipv6Policy: IPV6.includes(v.ipv6Policy as Ipv6Policy) ? (v.ipv6Policy as Ipv6Policy) : base.ipv6Policy,
    maliciousDomainBlock: typeof v.maliciousDomainBlock === "boolean" ? v.maliciousDomainBlock : base.maliciousDomainBlock,
  };
}
