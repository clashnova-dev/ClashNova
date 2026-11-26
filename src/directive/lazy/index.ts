import type { DirectiveBinding } from "vue";
import { useIntersectionObserver } from "@vueuse/core";

/**
 * Image lazy-load
 */
export const lazys: any = {
  // Current DOM element, image URL

  mounted(el: HTMLImageElement, binding: DirectiveBinding) {
    el.src = "@/assets/img/default.png";
    const { stop } = useIntersectionObserver(el, ([{ isIntersecting }]) => {
      if (isIntersecting) {
        el.src = binding.value;
        stop();
      }
    });
  },
};
