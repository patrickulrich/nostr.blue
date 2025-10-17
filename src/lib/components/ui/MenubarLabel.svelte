<script lang="ts">
  import { Menubar as MenubarPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    inset?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { inset = false, class: className, children }: Props = $props();
</script>

<MenubarPrimitive.GroupHeading
  class={cn('px-2 py-1.5 text-sm font-semibold', inset && 'pl-8', className)}
>
  {#if children}
    {@render children()}
  {/if}
</MenubarPrimitive.GroupHeading>
