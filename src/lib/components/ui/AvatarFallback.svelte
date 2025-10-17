<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<div class={cn('flex h-full w-full items-center justify-center rounded-full bg-muted', className)}>
  {#if children}
    {@render children()}
  {/if}
</div>
