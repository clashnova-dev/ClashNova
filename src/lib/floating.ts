// Shared geometry for floating UI (popovers, menus, tooltips). Callers keep their own anchoring
// strategy (centered, cursor, below-target) and use this only for the common viewport clamping.

/**
 * Clamp a floating element's desired top/left edge so it stays within the viewport, leaving
 * `margin` px on both sides.
 * @param desired  the intended top or left coordinate (px)
 * @param size     the element's height or width (px)
 * @param viewport the window's inner height or width (px)
 */
export function clampIntoViewport(desired: number, size: number, viewport: number, margin = 8): number {
  return Math.max(margin, Math.min(desired, viewport - size - margin));
}
