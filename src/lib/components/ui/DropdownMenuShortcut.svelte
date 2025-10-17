<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<span class={cn('ml-auto text-xs tracking-widest opacity-60', className)}>
  {#if children}
    {@render children()}
  {/if}
</span>
