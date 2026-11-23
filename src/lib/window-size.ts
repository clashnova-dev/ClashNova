export const MAIN_WINDOW = { width: 960, height: 670, minWidth: 820, minHeight: 650 };

export type MainWindowSize = { width: number; height: number; defaultWidth: number };

/** Apply a changed default once, then retain subsequent manual resizing. */
export function acceptMainWindow(value: unknown): MainWindowSize | null {
  const saved = value as Partial<MainWindowSize> | null;
  if (!Number.isFinite(saved?.width) || !Number.isFinite(saved?.height)) return null;
  return {
    width: saved!.defaultWidth === MAIN_WINDOW.width
      ? Math.max(MAIN_WINDOW.minWidth, Math.round(saved!.width!))
      : MAIN_WINDOW.width,
    height: Math.max(MAIN_WINDOW.minHeight, Math.round(saved!.height!)),
    defaultWidth: MAIN_WINDOW.width,
  };
}
