export type ConnectionRow = {
  id: string;
  metadata: {
    host: string; destinationIP: string; destinationPort: string; processPath: string;
    sourceIP?: string; sourcePort?: string; network?: string; type?: string;
  };
  upload: number; download: number; uploadSpeed: number; downloadSpeed: number;
  start: string; chains: string[]; rule?: string; rulePayload?: string;
};

export function connectionEndpoint(address: string, port?: string): string {
  if (!address) return '—';
  const host = address.includes(':') && !address.startsWith('[') ? `[${address}]` : address;
  return port ? `${host}:${port}` : host;
}

export type ConnectionSelection = { session: number; row: ConnectionRow };
/** Keep the latest observation through closure/history eviction, never across core sessions. */
export function refreshConnectionSelection(
  selected: ConnectionSelection | undefined,
  snapshot: { session: number; active: ConnectionRow[]; closed: ConnectionRow[] },
): ConnectionSelection | undefined {
  if (!selected || selected.session !== snapshot.session) return undefined;
  const row = snapshot.active.find(row => row.id === selected.row.id)
    || snapshot.closed.find(row => row.id === selected.row.id)
    || { ...selected.row, uploadSpeed: 0, downloadSpeed: 0 };
  return { session: selected.session, row };
}
