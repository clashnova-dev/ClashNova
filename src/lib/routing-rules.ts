import { CHINA_DOMAINS } from './china-domains.ts';

/** Built-in routing precedes provider rules; application policies precede both. */
export function builtinRouteRules(enabled: boolean) {
  return enabled ? [
    { ip_is_private: true, outbound: 'direct' },
    { domain_suffix: [...CHINA_DOMAINS], outbound: 'direct' },
  ] : [];
}
