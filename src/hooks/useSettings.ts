import { useNostr } from '@nostrify/react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useCurrentUser } from './useCurrentUser';
import { useEffect } from 'react';

export interface AppSettings {
  theme: 'light' | 'dark' | 'system';
  // Add more settings here as needed
}

const DEFAULT_SETTINGS: AppSettings = {
  theme: 'system',
};

const SETTINGS_D_TAG = 'nostr.blue:settings';

/**
 * Hook to manage user application settings stored on Nostr using kind 30078 events.
 * Provides settings retrieval, updates, and automatic theme application.
 * @returns Object containing current settings, loading state, and update mutation
 */
export function useSettings() {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();
  const queryClient = useQueryClient();

  // Fetch settings from Nostr
  const { data: settings = DEFAULT_SETTINGS, isLoading } = useQuery<AppSettings>({
    queryKey: ['settings', user?.pubkey],
    queryFn: async ({ signal }) => {
      if (!user) return DEFAULT_SETTINGS;

      try {
        const events = await nostr.query(
          [{ kinds: [30078], authors: [user.pubkey], '#d': [SETTINGS_D_TAG], limit: 1 }],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) }
        );

        const settingsEvent = events[0];
        if (!settingsEvent) return DEFAULT_SETTINGS;

        const parsed = JSON.parse(settingsEvent.content);
        return { ...DEFAULT_SETTINGS, ...parsed };
      } catch (error) {
        console.error('Failed to fetch settings:', error);
        return DEFAULT_SETTINGS;
      }
    },
    enabled: !!user,
    staleTime: Infinity, // Settings don't change often
  });

  // Save settings to Nostr
  const updateSettings = useMutation({
    mutationFn: async (newSettings: Partial<AppSettings>) => {
      if (!user) throw new Error('User not logged in');

      const updatedSettings = { ...settings, ...newSettings };

      const event = await user.signer.signEvent({
        kind: 30078,
        created_at: Math.floor(Date.now() / 1000),
        tags: [['d', SETTINGS_D_TAG]],
        content: JSON.stringify(updatedSettings),
      });

      await nostr.event(event, { signal: AbortSignal.timeout(5000) });

      return updatedSettings;
    },
    onSuccess: (updatedSettings) => {
      queryClient.setQueryData(['settings', user?.pubkey], updatedSettings);
    },
  });

  // Apply theme on mount and when settings change
  useEffect(() => {
    const applyTheme = () => {
      const root = document.documentElement;

      if (settings.theme === 'system') {
        const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        if (prefersDark) {
          root.classList.add('dark');
        } else {
          root.classList.remove('dark');
        }
      } else if (settings.theme === 'dark') {
        root.classList.add('dark');
      } else {
        root.classList.remove('dark');
      }
    };

    applyTheme();

    // Listen for system theme changes if using system theme
    if (settings.theme === 'system') {
      const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
      const handleChange = () => applyTheme();
      mediaQuery.addEventListener('change', handleChange);
      return () => mediaQuery.removeEventListener('change', handleChange);
    }
  }, [settings.theme]);

  return {
    settings,
    isLoading,
    updateSettings,
  };
}
