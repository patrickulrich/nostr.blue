<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<li class={cn('inline-flex items-center gap-1.5', className)}>
  {#if children}
    {@render children()}
  {/if}
</li>
