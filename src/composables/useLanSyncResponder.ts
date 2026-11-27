import { onMounted, onBeforeUnmount } from "vue";
import { confirmation } from "../lib/confirmation";
import { msg } from "../i18n";
import { useConnectionStore } from "../stores/connection";
import { useSubscriptionsStore } from "../stores/subscriptions";
import { notifications } from "../lib/notifications";
import type { LanPairRequest } from "../lib/lan-transfer";

// 被拉取方：全局监听「拉取同步」请求，复用全局确认弹窗（confirmation）询问用户，
// 接受则回传本机配置 payload。30s 未决自动取消（后端 decide_sync 同样 30s 超时）。
// 不建立配对、不新造弹窗（复用 ConfirmationHost）。
export function useLanSyncResponder() {
  const { bridge } = useConnectionStore();
  const subs = useSubscriptionsStore();
  let stop: (() => void) | null = null;
  let disposed = false;

  async function onRequest(req: LanPairRequest) {
    if (disposed) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const approved = await Promise.race<boolean>([
      confirmation.request({
        title: "lan.syncReqTitle",
        body: msg("lan.syncReqBody", { alias: req.peerAlias }),
        confirmLabel: "common.allow",
        details: ["lan.syncReqHint"],
      }),
      new Promise<boolean>(resolve => (timer = setTimeout(() => resolve(false), 30_000))),
    ]);
    if (timer) clearTimeout(timer);
    // 超时胜出时确认弹窗仍开着（单弹窗模型，占位的就是本请求），关掉它。
    if (!approved) confirmation.cancel();
    try {
      const b = await bridge();
      if (approved) await b.lanRespondSync(req.requestId, true, subs.buildLanSyncPayload());
      else await b.lanRespondSync(req.requestId, false, "");
    } catch (e) {
      notifications.show({
        body: e instanceof Error ? e.message : String(e),
        kind: "error",
        native: false,
      });
    }
  }

  onMounted(async () => {
    const b = await bridge();
    const unlisten = await b.onLanSync?.(req => void onRequest(req));
    if (disposed) unlisten?.();
    else stop = unlisten ?? null;
  });
  onBeforeUnmount(() => {
    disposed = true;
    stop?.();
  });
}
