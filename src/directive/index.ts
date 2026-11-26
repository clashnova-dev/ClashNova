import type { App } from "vue";

import { resize } from "./resize";
import { debounce } from "./debounce";
import { throttle } from "./throttle";
import { slide } from "./slide";
import { lazys } from "./lazy";
import { vContextMenu } from "./contextmenu";

// Register directives globally
function setupDirective(app: App<Element>) {
  /** Slide */
  app.directive("slide", slide);
  /** Window resize */
  app.directive("resize", resize);
  /** Debounce */
  app.directive("debounce", debounce);
  /** Throttle */
  app.directive("throttle", throttle);
  /** Image lazy-load */
  app.directive("lazys", lazys);
  /** Context menu */
  app.directive("context-menu", vContextMenu);
}

export default setupDirective;
