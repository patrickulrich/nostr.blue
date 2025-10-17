<script lang="ts">
  import { Pane } from 'paneforge';
  import { cn } from '$lib/utils';

  interface Props {
    defaultSize?: number;
    minSize?: number;
    maxSize?: number;
    collapsible?: boolean;
    collapsedSize?: number;
    order?: number;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let {
    defaultSize,
    minSize,
    maxSize,
    collapsible = false,
    collapsedSize,
    order,
    class: className,
    children
  }: Props = $props();
</script>

<Pane
  {defaultSize}
  {minSize}
  {maxSize}
  {collapsible}
  {collapsedSize}
  {order}
  class={className}
>
  {#if children}
    {@render children()}
  {/if}
</Pane>
