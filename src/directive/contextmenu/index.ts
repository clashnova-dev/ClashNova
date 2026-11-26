import type { DirectiveBinding } from "vue";

interface ContextMenuOptions {
  // Menu items
  options: Array<{ label: string; value: string }>;
  // Menu item callback
  callback: (value: string) => void;
  beforeShow?: () => void;
}

// Singleton storage
let menu: HTMLDivElement;
let styleEl: HTMLStyleElement;
let currentCallback: ContextMenuOptions["callback"] | null = null;
// Whether the menu has finished rendering
let initialized = false;

// Init: create the menu element, styles, and global click-to-hide
function initMenu(): void {
  if (initialized) return;
  // Create the menu DOM
  menu = document.createElement("div");
  menu.className = "context-menu";
  menu.style.display = "none";
  document.body.appendChild(menu);

  // Create and inject the styles
  styleEl = document.createElement("style");

  styleEl.textContent = `
       .context-menu {
           position: fixed;
           background: #ffffff;
           border: 1px solid var(--ui-border-light, #edf0f3);
           border-radius: 8px;
           min-width: 112px;
           padding: 6px;
           box-sizing: border-box;
           box-shadow: 0 12px 32px rgba(15, 23, 42, 0.16);
           z-index: 9999;
           font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
           opacity: 0;
           transform: scale(0.98);
           transform-origin: top left;
           transition: opacity 0.12s ease, transform 0.12s ease;
           user-select: none;
       }
       
       .context-menu .menu-item {
           height: 32px;
           padding: 0 12px;
           border-radius: 6px;
           display: flex;
           align-items: center;
           box-sizing: border-box;
           font-size: 14px;
           font-weight: normal;
           color: var(--ui-text-primary, #1f2933);
           cursor: pointer;
           transition: background-color 0.12s ease, color 0.12s ease;
           white-space: nowrap;
       }
       
       .context-menu .menu-item:hover {
           background-color: var(--ui-bg-muted, #f6f8fa);
           color: var(--content-active-color, #409eff);
       }
       
       .context-menu .menu-item:active {
           background-color: rgba(64, 158, 255, 0.12);
       }
       `;
  document.head.appendChild(styleEl);

  // Global click-to-hide (event delegation)
  document.addEventListener("click", handleGlobalClick);
  document.addEventListener("contextmenu", handleGlobalContextMenu);
  window.addEventListener("resize", hideMenu);
  window.addEventListener("wheel", hideMenu, { passive: true });
  menu.addEventListener("click", handleMenuClick);
  initialized = true;
}

// Global click handler
function handleGlobalClick(e: MouseEvent): void {
  if (menu && !menu.contains(e.target as Node)) {
    hideMenu();
  }
}

function handleGlobalContextMenu(e: MouseEvent): void {
  if (menu && !menu.contains(e.target as Node)) {
    hideMenu();
  }
}

function handleMenuClick(evt: MouseEvent): void {
  const target = (evt.target as HTMLElement).closest<HTMLElement>(".menu-item");
  if (!target || !menu.contains(target)) {
    return;
  }
  const val = target.dataset.value;
  // Grab the callback before hiding: hideMenu() nulls currentCallback, so hiding first loses it (click does nothing)
  const cb = currentCallback;
  hideMenu();
  if (val) {
    cb?.(val);
  }
}

// Render the context menu items
function renderMenu(options: ContextMenuOptions["options"]): void {
  menu.innerHTML = "";
  options.forEach(opt => {
    const item = document.createElement("div");
    item.className = "menu-item";
    item.dataset.value = opt.value;
    item.textContent = opt.label;
    menu.appendChild(item);
  });
}

// Compute the context menu position and show it
function showMenuAt(event: MouseEvent): void {
  const x = event.clientX;
  const y = event.clientY;
  menu.style.visibility = "hidden";
  menu.style.display = "block";
  menu.style.opacity = "0";
  menu.style.transform = "scale(0.98)";

  const { width, height } = menu.getBoundingClientRect();
  const margin = 8;
  const gap = 4;
  const container = (event.target as HTMLElement | null)?.closest?.(".message-records");
  const rect = container?.getBoundingClientRect();
  const minLeft = Math.max(margin, rect?.left ?? margin);
  const minTop = Math.max(margin, rect?.top ?? margin);
  const maxLeft = Math.min(window.innerWidth - margin, rect?.right ?? window.innerWidth - margin) - width;
  const maxTop = Math.min(window.innerHeight - margin, rect?.bottom ?? window.innerHeight - margin) - height;

  let left = x + gap;
  let top = y + gap;
  if (left > maxLeft) {
    left = x - width - gap;
  }
  if (top > maxTop) {
    top = y - height - gap;
  }
  left = Math.min(Math.max(minLeft, left), Math.max(minLeft, maxLeft));
  top = Math.min(Math.max(minTop, top), Math.max(minTop, maxTop));

  menu.style.left = `${left}px`;
  menu.style.top = `${top}px`;
  menu.style.visibility = "visible";
  requestAnimationFrame(() => {
    menu.style.opacity = "1";
    menu.style.transform = "scale(1)";
  });
}

function hideMenu(): void {
  if (!menu) return;
  menu.style.display = "none";
  currentCallback = null;
}

export const vContextMenu = {
  mounted(el: HTMLElement, binding: DirectiveBinding<ContextMenuOptions>) {
    initMenu();

    // Contextmenu event
    const handler = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const { options, callback, beforeShow } = binding.value;
      // Before-show hook
      beforeShow?.();
      // Render the menu items
      renderMenu(options);
      currentCallback = callback;
      // Where the menu appears
      showMenuAt(e);
    };

    // Register the contextmenu listener
    el.addEventListener("contextmenu", handler);
    (el as any).__contextMenuHandler__ = handler;
  },
  unmounted(el: HTMLElement) {
    // Clean up the event
    const handler = (el as any).__contextMenuHandler__;
    if (handler) el.removeEventListener("contextmenu", handler);
  },
};
