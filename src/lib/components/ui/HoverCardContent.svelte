<script lang="ts">
  import { LinkPreview } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    align?: 'start' | 'center' | 'end';
    sideOffset?: number;
    children?: import('svelte').Snippet;
  }

  let {
    class: className,
    align = 'center',
    sideOffset = 4,
    children
  }: Props = $props();
</script>

<LinkPreview.Portal>
  <LinkPreview.Content
    {align}
    {sideOffset}
    class={cn(
      'z-50 w-64 rounded-md border bg-popover p-4 text-popover-foreground shadow-md outline-none data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95',
      className
    )}
  >
    {#if children}
      {@render children()}
    {/if}
  </LinkPreview.Content>
</LinkPreview.Portal>
