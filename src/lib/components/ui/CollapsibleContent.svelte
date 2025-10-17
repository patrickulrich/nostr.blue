<script lang="ts">
  import { Collapsible as CollapsiblePrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';
  import { slide } from 'svelte/transition';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<CollapsiblePrimitive.Content forceMount class={cn('overflow-hidden', className)}>
  {#snippet child({ props, open })}
    {#if open}
      <div {...props} transition:slide={{ duration: 200 }}>
        {#if children}
          {@render children()}
        {/if}
      </div>
    {/if}
  {/snippet}
</CollapsiblePrimitive.Content>
