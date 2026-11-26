const DISTANCE = 35; // distance
const DURATION = 300; // duration

const map = new WeakMap(); // weak-reference map

/** Create the observer */
const ob = new IntersectionObserver((entries: any) => {
  for (const entry of entries) {
    if (entry.isIntersecting) {
      const animation = map.get(entry.target); // get the animation from the map
      if (animation) {
        animation.play(); // play the animation
        ob.unobserve(entry.target); // stop observing this element
      }
    }
  }
});

function isBelowViewport(el: any) {
  return el.getBoundingClientRect().top - DISTANCE > window.innerHeight; // check whether the element is below the viewport
}

export const slide: any = {
  mounted(el: any) {
    if (!isBelowViewport(el)) return; // if the element is within the viewport, skip the code below
    const animation = el.animate(
      [
        {
          transform: `translateY(${DISTANCE}px)`,
          opacity: 0.5,
        },
        {
          transform: "translateY(0)",
          opacity: 1,
        },
      ],
      {
        duration: DURATION, // animation duration
        easing: "ease-in-out", // animation easing
        fill: "forwards", // animation fill mode
      }
    );
    animation.pause(); // pause the animation
    ob.observe(el); // start observing this element
    map.set(el, animation); // store the animation object in the map
  },
  unmounted(el: any) {
    ob.unobserve(el); // stop observing this element
  },
};
