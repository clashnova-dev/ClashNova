export function captureShortcut(
  event: Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">
): string | null {
  if (!event.ctrlKey && !event.altKey && !event.metaKey) return null;
  if (!/^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-2]))$/.test(event.code)) return null;
  return [
    event.ctrlKey && "Control",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Super",
    event.code,
  ]
    .filter(Boolean)
    .join("+");
}
export function formatShortcut(value: string, mac: boolean) {
  return value
    .replace("CommandOrControl", mac ? "⌘" : "Ctrl")
    .replace(/Control/g, "Ctrl")
    .replace(/Super/g, mac ? "⌘" : "Win")
    .replace(/Alt/g, mac ? "⌥" : "Alt")
    .replace(/Key([A-Z])/g, "$1")
    .replace(/Digit([0-9])/g, "$1")
    .replaceAll("+", " + ");
}
