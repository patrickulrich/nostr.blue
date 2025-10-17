<script lang="ts">
  import { setContext } from 'svelte';
  import { cn } from '$lib/utils';

  interface Props {
    value: string;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { value, class: className, children }: Props = $props();

  setContext('accordion-item', { value });
</script>

<div class={cn('border-b', className)}>
  {#if children}
    {@render children()}
  {/if}
</div>
