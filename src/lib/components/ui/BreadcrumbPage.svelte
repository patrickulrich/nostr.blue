<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<span
  role="link"
  aria-disabled="true"
  aria-current="page"
  class={cn('font-normal text-foreground', className)}
>
  {#if children}
    {@render children()}
  {/if}
</span>
