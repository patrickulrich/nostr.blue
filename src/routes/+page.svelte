<script lang="ts">
  let count = $state(0);
</script>

<div class="min-h-screen flex items-center justify-center bg-background">
  <div class="text-center space-y-4">
    <h1 class="text-4xl font-bold">Welcome to mkstack (Svelte)</h1>
    <p class="text-muted-foreground">
      This is the starting point for your Svelte + Welshman conversion.
    </p>
    <button
      class="px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90"
      onclick={() => count++}
    >
      Count: {count}
    </button>
    <p class="text-sm text-muted-foreground">
      The React components from /src/pages and /src/components will be converted incrementally.
    </p>
  </div>
</div>
