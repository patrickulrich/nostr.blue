import { useSettings } from '@/hooks/useSettings';

interface ThemeProviderProps {
  children: React.ReactNode;
}

export function ThemeProvider({ children }: ThemeProviderProps) {
  // useSettings hook handles theme application via useEffect
  useSettings();

  return <>{children}</>;
}
