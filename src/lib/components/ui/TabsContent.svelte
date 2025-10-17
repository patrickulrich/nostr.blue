<script lang="ts">
  import { Tabs as TabsPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    value: string;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { value, class: className, children }: Props = $props();
</script>

<TabsPrimitive.Content
  {value}
  class={cn(
    'mt-2 ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
</TabsPrimitive.Content>
