// Node (server) data structure.
export type Node = {
  id: string;
  name: string;
  region: string;
  host: string;
  port: number;
  protocol: string;
  agentOnline: boolean;
  appliedConfigVersion: number;
  latestConfigVersion: number;
};
