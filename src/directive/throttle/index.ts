import type { Directive } from "vue";

/***
 *  Throttle: fires at most once per interval
 *  Fires immediately on the first call; the last call also fires regardless of whether the interval has elapsed
 * [accounts for the input's change event]
 *  @param {?Number|300} delay - interval in ms
 *  e.g. <el-button  v-throttle="2000" @click="reflush">>Refresh</el-button>
 */
export const throttle: Directive = {
  mounted(el: HTMLFieldSetElement, delay: any) {
    el.addEventListener("click", () => {
      // Disable click events on this directive's DOM element
      el.style.pointerEvents = "none";
      // Disable the button's click event
      el.disabled = true;
      // Grey out the button's styling
      el.style.backgroundColor = "#ccc";
      el.style.border = "none";
      el.style.color = "#666";
      el.style.cursor = "default";
      setTimeout(() => {
        // Re-enable click events on this directive's DOM element
        el.style.pointerEvents = "auto";
        el.disabled = false;
        el.style.backgroundColor = "";
        el.style.color = "";
        el.style.cursor = "";
      }, delay.value || 1000);
      // default 1 second
    });
  },
};
