<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<ol
  class={cn(
    'flex flex-wrap items-center gap-1.5 break-words text-sm text-muted-foreground sm:gap-2.5',
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
</ol>
