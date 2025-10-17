<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<h5 class={cn('mb-1 font-medium leading-none tracking-tight', className)}>
  {#if children}
    {@render children()}
  {/if}
</h5>
