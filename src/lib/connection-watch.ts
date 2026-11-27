const tickets = { summary: 0, details: 0 };

/** Tell the core whether anyone is looking at live traffic. A newer call wins if replies reorder. */
export async function setConnectionWatch(kind: "summary" | "details", on: boolean): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) return;
  const ticket = ++tickets[kind];
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("set_connection_watch", { kind, on, ticket });
}

/** Keep the watch matched to this page, including while the window is in the background. */
export function bindConnectionWatch(kind: "summary" | "details") {
  let stopped = false;
  const sync = () => void setConnectionWatch(kind, !stopped && !document.hidden).catch(() => {});
  return {
    start() {
      document.addEventListener("visibilitychange", sync);
      sync();
    },
    stop() {
      stopped = true;
      document.removeEventListener("visibilitychange", sync);
      sync();
    },
  };
}
