<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<h3 class={cn('text-2xl font-semibold leading-none tracking-tight', className)}>
  {#if children}
    {@render children()}
  {/if}
</h3>
