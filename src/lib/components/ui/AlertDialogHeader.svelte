<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<div class={cn('flex flex-col space-y-2 text-center sm:text-left', className)}>
  {#if children}
    {@render children()}
  {/if}
</div>
