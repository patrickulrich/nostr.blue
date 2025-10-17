<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<DialogPrimitive.Title class={cn('text-lg font-semibold leading-none tracking-tight', className)}>
  {#if children}
    {@render children()}
  {/if}
</DialogPrimitive.Title>
