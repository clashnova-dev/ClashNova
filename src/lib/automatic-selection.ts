import { CONNECTIVITY_CHECK_URL } from "../constants/domains.ts";

export const AUTOMATIC_NODE_ID = "airport:auto";
export const AUTOMATIC_OUTBOUND_TAG = "__airport_auto__";
export type Candidate = { id: string; agentOnline: boolean };
export function outboundTag(id: string, external: boolean) {
  return external ? id : `node-${id.replaceAll("-", "").slice(0, 8)}`;
}
/** The chain is the connection only while the chosen node is still its exit.
 *  Choosing another node drops it, so the node on screen is the one that connects. */
export function activeChain(chain: readonly string[], selected: string): string[] {
  if (chain.length < 2) return [];
  if (selected && selected !== chain[chain.length - 1]) return [];
  return [...chain];
}
/** Use the core's URLTest group, so selection continues while the window is hidden. */
export function configureSelection(config: any, selected: string, candidates: Candidate[], external: boolean) {
  const selector = config.outbounds?.find((item: any) => item.type === "selector" && item.tag === "select");
  if (!selector || !Array.isArray(selector.outbounds)) throw new Error("subscriptions.selectedNodeMissing");
  if (selected !== AUTOMATIC_NODE_ID) {
    const tag = outboundTag(selected, external);
    if (!selector.outbounds.includes(tag)) throw new Error("subscriptions.selectedNodeMissing");
    selector.default = tag;
    return;
  }
  const tags = candidates
    .filter(node => node.agentOnline)
    .map(node => outboundTag(node.id, external))
    .filter(tag => selector.outbounds.includes(tag));
  if (!tags.length) throw new Error("nodes.noneAvailable");
  if (config.outbounds.some((item: any) => item.tag === AUTOMATIC_OUTBOUND_TAG))
    throw new Error("error.subscriptionTagAuto");
  config.outbounds.push({
    type: "urltest",
    tag: AUTOMATIC_OUTBOUND_TAG,
    outbounds: tags,
    url: CONNECTIVITY_CHECK_URL,
    interval: "3m",
    tolerance: 50,
    idle_timeout: "30m",
    interrupt_exist_connections: false,
  });
  selector.outbounds = [AUTOMATIC_OUTBOUND_TAG, ...selector.outbounds];
  selector.default = AUTOMATIC_OUTBOUND_TAG;
}
