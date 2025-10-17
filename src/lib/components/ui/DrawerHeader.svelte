<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<div class={cn('grid gap-1.5 p-4 text-center sm:text-left', className)}>
  {#if children}
    {@render children()}
  {/if}
</div>
