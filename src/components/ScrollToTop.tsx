import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';

/**
 * Component that scrolls window to top when route pathname changes.
 * Ensures new pages start at the top of the viewport.
 */
export function ScrollToTop() {
  const { pathname } = useLocation();

  useEffect(() => {
    window.scrollTo(0, 0);
  }, [pathname]);

  return null;
}