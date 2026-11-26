const map = new WeakMap();

// Create a ResizeObserver instance
const ob = new ResizeObserver((entries: any[]) => {
  // Iterate over all observed elements
  for (const entry of entries) {
    // Get the handler for this element
    const handler = map.get(entry.target);
    // If a handler exists
    if (handler) {
      // Call the handler with the element's width and height
      handler({
        width: entry.borderBoxSize[0].inlineSize,
        height: entry.borderBoxSize[0].blockSize,
      });
    }
  }
});

/**
 * Element resize directive
 */
export const resize: any = {
  mounted(el: any, binding: any) {
    // Observe size changes of the el element
    map.set(el, binding.value);
    ob.observe(el);
  },
  unmounted(el: any) {
    // Stop observing
    ob.unobserve(el);
  },
};
