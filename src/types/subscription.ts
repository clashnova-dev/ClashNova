import { type SubscriptionUsage } from "../lib/subscription-info";

// External subscription (a locally imported profile).
export type ExternalProfile = {
  id: string;
  name: string;
  url: string;
  raw: string;
  updatedAt: string;
  usage?: SubscriptionUsage;
};

// The currently active subscription for the airport plan.
export type ActiveSubscription = {
  planName: string;
  usedChargedBytes: number;
  limitBytes: number;
  expiresAt?: string | null;
  subscriptionUrl?: string;
};
