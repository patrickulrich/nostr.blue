<script lang="ts">
  import '@fontsource-variable/inter';
  import '../index.css';
  import '../lib/polyfills.ts';

  import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
  import { onMount, onDestroy } from 'svelte';
  import { afterNavigate } from '$app/navigation';
  import { appConfig, applyTheme, setupThemeWatcher } from '$lib/stores/appStore';
  import { welshmanRouter } from '$lib/stores/welshman';
  import { setupWelshmanPersistence } from '$lib/stores/welshmanPersistence';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();

  // Scroll to top on navigation
  afterNavigate(() => {
    window.scrollTo(0, 0);
  });

  // Query client setup
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        refetchOnWindowFocus: false,
        staleTime: 60000, // 1 minute
        gcTime: Infinity,
        retry: 1,
      }
    }
  });

  // Apply theme on mount and setup watcher
  onMount(() => {
    applyTheme($appConfig.theme);

    // Set up Welshman persistence BEFORE initializing router
    // This loads persisted pubkey/sessions from localStorage
    const cleanupPersistence = setupWelshmanPersistence();

    welshmanRouter.init();
    const cleanupTheme = setupThemeWatcher();

    // Return combined cleanup function
    return () => {
      if (cleanupPersistence) cleanupPersistence();
      if (cleanupTheme) cleanupTheme();
    };
  });

  // Cleanup Welshman on unmount
  onDestroy(() => {
    welshmanRouter.cleanup();
  });

  // Apply theme when config changes
  $effect(() => {
    applyTheme($appConfig.theme);
  });
</script>

<QueryClientProvider client={queryClient}>
  {#if children}
    {@render children()}
  {/if}
</QueryClientProvider>
