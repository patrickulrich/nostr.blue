<script lang="ts">
  import '@fontsource-variable/inter';
  import '../index.css';
  import '../lib/polyfills.ts';

  import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
  import { onMount, onDestroy } from 'svelte';
  import { afterNavigate } from '$app/navigation';
  import { appConfig, applyTheme, setupThemeWatcher } from '$lib/stores/appStore';
  import { welshmanRouter } from '$lib/stores/welshman';

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
      }
    }
  });

  // Apply theme on mount and setup watcher
  onMount(() => {
    applyTheme($appConfig.theme);
    welshmanRouter.init();
    return setupThemeWatcher();
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
