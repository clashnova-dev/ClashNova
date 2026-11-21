import vectors from "../../contracts/connection-state.vectors.json";

export type Phase = (typeof vectors.states)[number];
export type ConnEvent = (typeof vectors.events)[number];

const table = new Map(vectors.transitions.map((item) => [`${item.from}|${item.event}`, item.to as Phase]));

export function reduce(from: Phase, event: ConnEvent): Phase {
  const next = table.get(`${from}|${event}`);
  if (!next) throw new Error(`Invalid connection transition: ${from} + ${event}`);
  return next;
}

export const phases = vectors.states;
