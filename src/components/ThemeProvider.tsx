import { useSettings } from '@/hooks/useSettings';

interface ThemeProviderProps {
  children: React.ReactNode;
}

/**
 * Theme provider component that applies user theme preferences.
 * Initializes the settings hook to apply theme to document root.
 * @param props - Provider props containing children elements
 */
export function ThemeProvider({ children }: ThemeProviderProps) {
  // useSettings hook handles theme application via useEffect
  useSettings();

  return <>{children}</>;
}
