import type { Directive, DirectiveBinding } from "vue";

/***
 * Debounce: only the last call within the interval fires
 *  @param {?Number|300} time - interval in ms
 *  @param {Function} fn - handler to run
 *  @param {?String|"click"} event - event type, e.g. "click"
 *  @param {Array} binding.value - [fn,event,time]
 *  @author aChuan
 *  e.g. <el-button v-debounce="[reset,`click`,300]">Refresh</el-button>
 *  or shorthand: <el-button v-debounce="[reset]">Refresh</el-button>
 */
export const debounce: Directive = {
  mounted(el: HTMLElement, binding: DirectiveBinding) {
    const [fn, event = "click", time = 300] = binding.value;
    let timer: NodeJS.Timeout;
    el.addEventListener(event, () => {
      // setTimeout returns timeoutID, a positive integer identifying the timer. This value can be passed to clearTimeout() to cancel it.
      // On the same object (a window or worker), subsequent setTimeout()/setInterval() calls never reuse the same timer id. Different objects use separate id pools.
      if (timer) clearTimeout(timer);
      // After clearTimeout() destroys the timer, the timer variable's value is not cleared
      timer = setTimeout(() => fn(), time);
    });
  },
};
