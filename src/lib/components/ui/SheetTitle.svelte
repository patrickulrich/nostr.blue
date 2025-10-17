<script lang="ts">
  import { Dialog as SheetPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    level?: 1 | 2 | 3 | 4 | 5 | 6;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { level = 3, class: className, children }: Props = $props();
</script>

<SheetPrimitive.Title {level} class={cn('text-lg font-semibold text-foreground', className)}>
  {#if children}
    {@render children()}
  {/if}
</SheetPrimitive.Title>
