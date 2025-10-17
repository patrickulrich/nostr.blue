<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    for?: string;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { for: htmlFor, class: className, children }: Props = $props();
</script>

<label
  for={htmlFor}
  class={cn(
    'text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70',
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
</label>
