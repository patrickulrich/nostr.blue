<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<div class={cn('relative flex h-10 w-10 shrink-0 overflow-hidden rounded-full', className)}>
  {#if children}
    {@render children()}
  {/if}
</div>
