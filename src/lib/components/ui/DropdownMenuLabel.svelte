<script lang="ts">
  import { DropdownMenu as DropdownMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    inset?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { inset = false, class: className, children }: Props = $props();
</script>

<DropdownMenuPrimitive.GroupHeading
  class={cn('px-2 py-1.5 text-sm font-semibold', inset && 'pl-8', className)}
>
  {#if children}
    {@render children()}
  {/if}
</DropdownMenuPrimitive.GroupHeading>
