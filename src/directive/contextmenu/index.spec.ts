// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DirectiveBinding } from "vue";
import { vContextMenu } from "./index";

type MenuOptions = {
  options: Array<{ label: string; value: string }>;
  callback: (value: string) => void;
  beforeShow?: () => void;
};

const bind = (value: MenuOptions) => ({ value }) as DirectiveBinding<MenuOptions>;

const menuEl = () => document.querySelector<HTMLElement>(".context-menu");
const menuItems = () => Array.from(document.querySelectorAll<HTMLElement>(".context-menu .menu-item"));

/** Dispatch a right-click on the target element to open the menu */
const openMenu = (el: HTMLElement) => {
  el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
};

/** Click a menu item (bubbles to the delegated listener on .context-menu) */
const clickItem = (item: HTMLElement) => {
  item.dispatchEvent(new MouseEvent("click", { bubbles: true }));
};

describe("v-context-menu 指令", () => {
  let el: HTMLElement;
  let callback: ReturnType<typeof vi.fn>;
  let beforeShow: ReturnType<typeof vi.fn>;

  // The directive's menu DOM is a module-level singleton (initMenu runs once and appends to body),
  // so we can't clear body—otherwise the singleton handle points to a detached node and later cases can't find the menu.
  beforeEach(() => {
    el = document.createElement("div");
    document.body.appendChild(el);

    callback = vi.fn();
    beforeShow = vi.fn();

    vContextMenu.mounted(
      el,
      bind({
        options: [
          { label: "回复", value: "reply" },
          { label: "删除", value: "delete" },
        ],
        callback,
        beforeShow,
      })
    );
  });

  afterEach(() => {
    vContextMenu.unmounted(el);
    el.remove();
    // Fire a global click (target outside the menu) to dismiss any leftover menu and isolate cases
    document.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  });

  it("右键时调用 beforeShow 并按 options 渲染菜单项", () => {
    openMenu(el);

    expect(beforeShow).toHaveBeenCalledOnce();
    expect(menuEl()?.style.display).toBe("block");
    expect(menuItems().map(i => i.textContent)).toEqual(["回复", "删除"]);
    expect(menuItems().map(i => i.dataset.value)).toEqual(["reply", "delete"]);
  });

  // Regression: hideMenu() inside handleMenuClick nulls out currentCallback;
  // if hideMenu() runs before invoking currentCallback, clicking any menu item silently does nothing.
  it("点击菜单项必须触发回调，并带上该项的 value", () => {
    openMenu(el);
    clickItem(menuItems()[0]);

    expect(callback).toHaveBeenCalledExactlyOnceWith("reply");
  });

  it("点击后菜单隐藏", () => {
    openMenu(el);
    clickItem(menuItems()[1]);

    expect(callback).toHaveBeenCalledExactlyOnceWith("delete");
    expect(menuEl()?.style.display).toBe("none");
  });

  it("菜单隐藏后再点击（无当前回调）不应重复触发", () => {
    openMenu(el);
    const [firstItem] = menuItems();
    clickItem(firstItem);
    expect(callback).toHaveBeenCalledTimes(1);

    // Menu is hidden and currentCallback is cleared; clicking the leftover DOM node again shouldn't fire
    clickItem(firstItem);
    expect(callback).toHaveBeenCalledTimes(1);
  });

  it("每次右键都取当前 binding 的 options / callback（多次打开互不串台）", () => {
    openMenu(el);
    clickItem(menuItems()[0]);
    expect(callback).toHaveBeenCalledExactlyOnceWith("reply");

    openMenu(el);
    clickItem(menuItems()[1]);
    expect(callback).toHaveBeenCalledTimes(2);
    expect(callback).toHaveBeenLastCalledWith("delete");
  });

  it("unmounted 后右键不再弹出菜单", () => {
    vContextMenu.unmounted(el);
    openMenu(el);

    expect(beforeShow).not.toHaveBeenCalled();
  });
});
