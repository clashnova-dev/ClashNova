export const DEFAULT_PROXY_PORT = 2080;
export const MIN_PROXY_PORT = 1024;
export const MAX_PROXY_PORT = 65535;

export const TUN_STACKS = ["mixed", "system", "gvisor"] as const;
export type TunStack = (typeof TUN_STACKS)[number];
export const DEFAULT_TUN_MTU = 1500;
export const MIN_TUN_MTU = 1280;
export const MAX_TUN_MTU = 9000;
export const MAX_TUN_EXCLUSIONS = 64;

export const DNS_MODES = ["fakeip", "remote"] as const;
export type DnsMode = (typeof DNS_MODES)[number];
export const DNS_REMOTES = ["cloudflare", "google", "quad9"] as const;
export type DnsRemote = (typeof DNS_REMOTES)[number];
export const DNS_DOMESTICS = ["local", "ali", "dnspod"] as const;
export type DnsDomestic = (typeof DNS_DOMESTICS)[number];

/** What a user may adjust about the virtual network device. The native side validates again. */
export type TunSettings = {
  stack: TunStack;
  mtu: number;
  strictRoute: boolean;
  ipv6: boolean;
  /** Linux only; ignored elsewhere. */
  autoRedirect: boolean;
  routeExclude: string[];
  /** Block the network when the tunnel drops unexpectedly, until it is back or released. */
  killSwitch: boolean;
  /** DNS 覆写: fake-ip split (default) or real resolution through the node's DoH. */
  dnsMode: DnsMode;
  /** Foreign DoH preset, used in remote mode. */
  dnsRemote: DnsRemote;
  /** Domestic/direct resolver preset. */
  dnsDomestic: DnsDomestic;
};

export const defaultTunSettings = (): TunSettings => ({
  stack: "mixed", mtu: DEFAULT_TUN_MTU, strictRoute: true, ipv6: false, autoRedirect: false, routeExclude: [], killSwitch: false,
  dnsMode: "fakeip", dnsRemote: "cloudflare", dnsDomestic: "local",
});

export type NetworkSettings = { proxyPort: number; allowLan: boolean; tun: TunSettings };

export function validateTunMtu(value: string | number): number {
  const text = String(value).trim();
  const mtu = Number(text);
  if (!/^\d+$/.test(text) || !Number.isSafeInteger(mtu) || mtu < MIN_TUN_MTU || mtu > MAX_TUN_MTU) {
    throw new Error("settings.tunMtuInvalid");
  }
  return mtu;
}

function isIpv4(address: string): boolean {
  const parts = address.split(".");
  return parts.length === 4 && parts.every(part => /^\d{1,3}$/.test(part) && Number(part) <= 255);
}

function isIpv6(address: string): boolean {
  if (!/^[0-9a-fA-F:.]+$/.test(address) || !address.includes(":")) return false;
  const halves = address.split("::");
  if (halves.length > 2) return false;
  const groups = halves.flatMap(half => (half ? half.split(":") : []));
  // An embedded IPv4 tail counts as two groups.
  const tail = groups.length && groups[groups.length - 1].includes(".") ? groups.pop()! : "";
  if (tail && !isIpv4(tail)) return false;
  if (!groups.every(group => /^[0-9a-fA-F]{1,4}$/.test(group))) return false;
  const count = groups.length + (tail ? 2 : 0);
  return halves.length === 2 ? count <= 7 : count === 8;
}

/** An IP prefix such as `192.168.0.0/16` or `fc00::/7`. */
export function isCidr(value: string): boolean {
  const slash = value.indexOf("/");
  if (slash <= 0) return false;
  const address = value.slice(0, slash), prefix = value.slice(slash + 1);
  if (!/^\d{1,3}$/.test(prefix)) return false;
  if (isIpv4(address)) return Number(prefix) <= 32;
  return isIpv6(address) && Number(prefix) <= 128;
}

/** One prefix per line (commas and spaces also separate); duplicates are dropped. */
export function parseRouteExclude(text: string): string[] {
  const prefixes = [...new Set(text.split(/[\s,]+/).map(item => item.trim()).filter(Boolean))];
  if (prefixes.length > MAX_TUN_EXCLUSIONS || !prefixes.every(isCidr)) throw new Error("settings.tunExcludeInvalid");
  return prefixes;
}

export function validateProxyPort(value: string | number): number {
  const text = String(value).trim();
  const port = Number(text);
  if (!/^\d+$/.test(text) || !Number.isSafeInteger(port) || port < MIN_PROXY_PORT || port > MAX_PROXY_PORT) {
    throw new Error("settings.portInvalid");
  }
  return port;
}

export function proxyAddress(port: number): string {
  return `127.0.0.1:${validateProxyPort(port)}`;
}
