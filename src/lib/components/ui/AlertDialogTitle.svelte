<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<h2 class={cn('text-lg font-semibold', className)}>
  {#if children}
    {@render children()}
  {/if}
</h2>
