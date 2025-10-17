<script lang="ts">
  import '@fontsource-variable/inter';
  import '../index.css';
  import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';

  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 1000 * 60 * 5, // 5 minutes
      }
    }
  });
</script>

<QueryClientProvider client={queryClient}>
  <slot />
</QueryClientProvider>
