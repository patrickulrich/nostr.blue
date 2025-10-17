import { onMount } from 'svelte';

const MOBILE_BREAKPOINT = 768;

export function useIsMobile() {
  let isMobile = $state(false);

  onMount(() => {
    const checkMobile = () => {
      isMobile = window.innerWidth < MOBILE_BREAKPOINT;
    };

    checkMobile();

    const mediaQuery = window.matchMedia(`(max-width: ${MOBILE_BREAKPOINT - 1}px)`);

    const handleChange = (e: MediaQueryListEvent | MediaQueryList) => {
      isMobile = e.matches;
    };

    // Modern browsers
    if (mediaQuery.addEventListener) {
      mediaQuery.addEventListener('change', handleChange);

      return () => {
        mediaQuery.removeEventListener('change', handleChange);
      };
    } else {
      // Fallback for older browsers
      mediaQuery.addListener(handleChange);

      return () => {
        mediaQuery.removeListener(handleChange);
      };
    }
  });

  return {
    get isMobile() {
      return isMobile;
    }
  };
}
