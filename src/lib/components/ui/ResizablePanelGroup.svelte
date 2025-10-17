<script lang="ts">
  import { PaneGroup } from 'paneforge';
  import { cn } from '$lib/utils';

  interface Props {
    direction?: 'horizontal' | 'vertical';
    autoSaveId?: string;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { direction = 'horizontal', autoSaveId, class: className, children }: Props = $props();
</script>

<PaneGroup
  {direction}
  {autoSaveId}
  class={cn('flex h-full w-full data-[direction=vertical]:flex-col', className)}
>
  {#if children}
    {@render children()}
  {/if}
</PaneGroup>
