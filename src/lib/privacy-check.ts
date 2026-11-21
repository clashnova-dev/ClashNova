import type { Text } from '../i18n';
export type PrivacyTone = 'idle' | 'ok' | 'warn' | 'bad';
export type RtcObservation = { kind: 'off' | 'none' | 'lan' | 'public' | 'timeout'; ips?: string[] };
export function normalizeIp(value: string): string {
  const ip = value.trim().replace(/^\[|\]$/g, '');
  if (ip.includes(':')) {
    try { const host = new URL(`http://[${ip}]/`).hostname.toLowerCase();
      const mapped = host.match(/^\[::ffff:([0-9a-f]+):([0-9a-f]+)\]$/);
      if (mapped) {
        const high = parseInt(mapped[1], 16), low = parseInt(mapped[2], 16);
        return [high >> 8, high & 255, low >> 8, low & 255].join('.');
      }
      return host; } catch { return ''; }
  }
  const parts = ip.split('.');
  if (parts.length !== 4 || parts.some(p => !/^\d{1,3}$/.test(p) || +p > 255)) return '';
  return parts.map(Number).join('.');
}
export function isPublicIp(value: string): boolean {
  const normalized = normalizeIp(value);
  if (!normalized) return false;
  if (normalized.startsWith('[')) {
    // Global-unicast IPv6. Private, unspecified, loopback and multicast are excluded.
    return /^[23][0-9a-f]{0,3}:/.test(normalized.slice(1)) && !normalized.startsWith('[2001:db8:');
  }
  const [a,b,c] = normalized.split('.').map(Number);
  return a > 0 && a < 224 && a !== 10 && a !== 127 && !(a === 169 && b === 254)
    && !(a === 172 && b >= 16 && b <= 31) && !(a === 192 && b === 168)
    && !(a === 100 && b >= 64 && b <= 127) && !(a === 198 && (b === 18 || b === 19))
    && !(a === 192 && b === 0 && (c === 0 || c === 2))
    && !(a === 198 && b === 51 && c === 100) && !(a === 203 && b === 0 && c === 113);
}
export type PrivacyEvidence = { connected: boolean; proxy: string; mode: string; exitIp: string; directIp?: string; ipv6Ip: string; proxyIpv6Ip?: string };
export function ipv6Assessment(native: PrivacyEvidence): { text: Text; tone: PrivacyTone } {
  if (!isPublicIp(native.ipv6Ip)) return { text: 'check.ipv6None', tone: 'idle' };
  if (!native.connected) return { text: 'check.ipv6Present', tone: 'idle' };
  if (native.proxy === 'ok' && isPublicIp(native.proxyIpv6Ip || '') && normalizeIp(native.ipv6Ip) === normalizeIp(native.proxyIpv6Ip!))
    return { text: 'check.ipv6Through', tone: 'ok' };
  return { text: 'check.ipv6Bypass', tone: 'warn' };
}
export function webrtcAssessment(rtc: RtcObservation, native?: PrivacyEvidence): { text: Text; tone: PrivacyTone } {
  if (rtc.kind === 'off') return { text: 'check.webrtcOff', tone: 'idle' };
  const ips = (rtc.ips || []).filter(isPublicIp);
  if (rtc.kind === 'lan' && !ips.length) return { text: 'check.webrtcLan', tone: 'idle' };
  if (!ips.length) return { text: rtc.kind === 'timeout' ? 'check.webrtcTimeout' : 'check.webrtcNone', tone: 'idle' };
  if (!native?.connected || native.proxy !== 'ok' || !normalizeIp(native.exitIp)) return { text: { key:'check.webrtcUnverified', values:{ip:ips.join(', ')} }, tone:'warn' };
  const exits = [native.exitIp, native.proxyIpv6Ip || ''].filter(isPublicIp).map(normalizeIp);
  const different = ips.filter(ip => !exits.includes(normalizeIp(ip)));
  if (!different.length) return { text: 'check.webrtcMatches', tone:'ok' };
  const direct = native.mode !== 'tun' ? [native.directIp, native.ipv6Ip].filter(Boolean).map(ip => normalizeIp(ip!)) : [];
  const bypass = different.find(ip => direct.includes(normalizeIp(ip)));
  if (bypass) return { text: {key:'check.webrtcPublic',values:{ip:bypass}}, tone:'bad' };
  return { text: {key:'check.webrtcDifferent',values:{ip:different.join(', ')}}, tone:'warn' };
}
