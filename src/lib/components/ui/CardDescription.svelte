<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<p class={cn('text-sm text-muted-foreground', className)}>
  {#if children}
    {@render children()}
  {/if}
</p>
