<script lang="ts">
  import { page } from '$app/stores';
  import Button from '$lib/components/ui/Button.svelte';

  function handleReset() {
    window.location.href = '/';
  }

  function handleReload() {
    window.location.reload();
  }

  $: error = $page.error;
  $: status = $page.status;
</script>

<div class="min-h-screen bg-background flex items-center justify-center p-4">
  <div class="max-w-md w-full space-y-4">
    <div class="text-center">
      <h2 class="text-2xl font-bold text-foreground mb-2">
        {#if status === 404}
          Page not found
        {:else}
          Something went wrong
        {/if}
      </h2>
      <p class="text-muted-foreground">
        {#if status === 404}
          The page you're looking for doesn't exist.
        {:else}
          An unexpected error occurred. The error has been reported.
        {/if}
      </p>
    </div>

    {#if error}
      <div class="bg-muted p-4 rounded-lg">
        <details class="text-sm">
          <summary class="cursor-pointer font-medium text-foreground">
            Error details
          </summary>
          <div class="mt-2 space-y-2">
            <div>
              <strong class="text-foreground">Message:</strong>
              <p class="text-muted-foreground mt-1">
                {error.message || 'Unknown error'}
              </p>
            </div>
            {#if 'stack' in error && error.stack}
              <div>
                <strong class="text-foreground">Stack trace:</strong>
                <pre class="text-xs text-muted-foreground mt-1 overflow-auto max-h-32">{error.stack}</pre>
              </div>
            {/if}
          </div>
        </details>
      </div>
    {/if}

    <div class="flex gap-2">
      <Button onclick={handleReset} variant="default" class="flex-1">
        Go home
      </Button>
      <Button onclick={handleReload} variant="secondary" class="flex-1">
        Reload page
      </Button>
    </div>
  </div>
</div>
