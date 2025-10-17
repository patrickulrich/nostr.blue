import { useEffect, useState } from "react"

const MOBILE_BREAKPOINT = 768;

/**
 * Hook to detect if the viewport is in mobile size (< 768px).
 * Updates reactively when window is resized.
 *
 * @returns True if viewport width is below mobile breakpoint
 */
export function useIsMobile(): boolean {
  const [isMobile, setIsMobile] = useState(() =>
    typeof window !== 'undefined' ? window.innerWidth < MOBILE_BREAKPOINT : false
  );

  useEffect(() => {
    const mql = window.matchMedia(`(max-width: ${MOBILE_BREAKPOINT - 1}px)`);
    const onChange = () => {
      setIsMobile(window.innerWidth < MOBILE_BREAKPOINT);
    }
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, []);

  return isMobile;
}
