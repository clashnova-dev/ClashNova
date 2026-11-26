export type SubscriptionUsage = {
  upload?: number;
  download?: number;
  total?: number;
  expire?: number;
};
export type SubscriptionDownload = {
  raw: string;
  userinfo: string | null;
  metadataSupported?: boolean;
};

export function parseSubscriptionUsage(header: string | null | undefined): SubscriptionUsage | undefined {
  if (!header || header.length > 4096) return undefined;
  const info: SubscriptionUsage = {};
  for (const key of ["upload", "download", "total", "expire"] as const) {
    const matches = header
      .split(";")
      .map(part => part.trim().split("="))
      .filter(parts => parts[0]?.toLowerCase() === key);
    if (matches.length !== 1 || matches[0].length !== 2 || !/^\d+$/.test(matches[0][1].trim())) continue;
    const value = Number(matches[0][1].trim());
    if (Number.isSafeInteger(value) && value >= 0 && (key !== "expire" || value <= 8640000000000)) info[key] = value;
  }
  return Object.keys(info).length ? info : undefined;
}
export function usedTraffic(info?: SubscriptionUsage): number | undefined {
  if (
    !Number.isSafeInteger(info?.upload) ||
    !Number.isSafeInteger(info?.download) ||
    info!.upload! < 0 ||
    info!.download! < 0
  )
    return undefined;
  const used = info!.upload! + info!.download!;
  return Number.isSafeInteger(used) ? used : undefined;
}
export function trafficPercent(info?: SubscriptionUsage): number | undefined {
  const used = usedTraffic(info);
  return used === undefined || !Number.isSafeInteger(info?.total) || info!.total! <= 0
    ? undefined
    : Math.min(100, Math.round((used / info!.total!) * 100));
}
export function subscriptionHost(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}
