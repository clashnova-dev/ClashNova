import { shallowRef, readonly } from "vue";
import type { MessageKey, Text } from "../i18n/index.ts";

export type Confirmation = {
  title: Text;
  body: Text;
  confirmLabel?: MessageKey;
  danger?: boolean;
  details?: Text[];
};

// A single modal, no queued approvals. A stale close/double-click cannot approve another action.
export function createConfirmation() {
  const current = shallowRef<(Confirmation & { id: number }) | null>(null);
  let sequence = 0;
  let resolve: ((approved: boolean) => void) | undefined;
  return {
    current: readonly(current),
    request(options: Confirmation): Promise<boolean> {
      if (current.value) return Promise.resolve(false);
      return new Promise<boolean>(finish => {
        resolve = finish;
        current.value = { ...options, id: ++sequence };
      });
    },
    settle(id: number, approved: boolean) {
      if (current.value?.id !== id) return;
      const finish = resolve;
      resolve = undefined;
      current.value = null;
      finish?.(approved === true);
    },
    cancel() {
      if (current.value) this.settle(current.value.id, false);
    },
  };
}
export const confirmation = createConfirmation();
