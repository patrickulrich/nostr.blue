<script lang="ts">
  import '@fontsource-variable/inter';
  import '../index.css';
  import '../lib/polyfills.ts';

  import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
  import { onMount } from 'svelte';

  // App configuration types
  type Theme = "dark" | "light" | "system";

  interface AppConfig {
    theme: Theme;
    relayUrl: string;
  }

  // Default configuration
  const defaultConfig: AppConfig = {
    theme: "light",
    relayUrl: "wss://relay.ditto.pub",
  };

  // Preset relays
  const presetRelays = [
    { url: 'wss://relay.ditto.pub', name: 'Ditto' },
    { url: 'wss://relay.nostr.band', name: 'Nostr.Band' },
    { url: 'wss://relay.damus.io', name: 'Damus' },
    { url: 'wss://relay.primal.net', name: 'Primal' },
  ];

  // Query client setup
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        refetchOnWindowFocus: false,
        staleTime: 60000, // 1 minute
        gcTime: Infinity,
      }
    }
  });

  // App config state - will be enhanced with proper store later
  let config = $state<AppConfig>(defaultConfig);

  // Load config from localStorage on mount
  onMount(() => {
    const stored = localStorage.getItem('nostr:app-config');
    if (stored) {
      try {
        const parsed = JSON.parse(stored);
        config = { ...defaultConfig, ...parsed };
      } catch (e) {
        console.error('Failed to parse app config:', e);
      }
    }

    // Apply initial theme
    applyTheme(config.theme);

    // Watch for system theme changes
    if (config.theme === 'system') {
      const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
      const handleChange = () => applyTheme('system');
      mediaQuery.addEventListener('change', handleChange);

      return () => mediaQuery.removeEventListener('change', handleChange);
    }
  });

  // Apply theme to document
  function applyTheme(theme: Theme) {
    const root = window.document.documentElement;
    root.classList.remove('light', 'dark');

    if (theme === 'system') {
      const systemTheme = window.matchMedia('(prefers-color-scheme: dark)').matches
        ? 'dark'
        : 'light';
      root.classList.add(systemTheme);
    } else {
      root.classList.add(theme);
    }
  }

  // Save config to localStorage when it changes
  $effect(() => {
    localStorage.setItem('nostr:app-config', JSON.stringify(config));
    applyTheme(config.theme);
  });
</script>

<QueryClientProvider client={queryClient}>
  <slot />
</QueryClientProvider>
