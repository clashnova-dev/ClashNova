export function sortByLatency<T extends { id: string }>(nodes: T[], delays: Record<string, number>): T[] {
  return [...nodes].sort((a, b) => {
    const first = delays[a.id], second = delays[b.id];
    if (first === undefined) return second === undefined ? 0 : 1;
    if (second === undefined) return -1;
    return first - second;
  });
}
