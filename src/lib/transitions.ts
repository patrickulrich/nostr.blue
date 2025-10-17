import { cubicOut } from 'svelte/easing';
import type { TransitionConfig } from 'svelte/transition';

export function flyAndScale(
  node: Element,
  params: { y?: number; start?: number; duration?: number } = {}
): TransitionConfig {
  const { y = -8, start = 0.95, duration = 150 } = params;

  const style = getComputedStyle(node);
  const transform = style.transform === 'none' ? '' : style.transform;

  return {
    duration,
    delay: 0,
    css: (t) => {
      const eased = cubicOut(t);
      return `
        transform: ${transform} translateY(${(1 - eased) * y}px) scale(${start + eased * (1 - start)});
        opacity: ${eased};
      `;
    }
  };
}
