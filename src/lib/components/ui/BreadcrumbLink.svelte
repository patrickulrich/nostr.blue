<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    href?: string;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { href, class: className, children }: Props = $props();
</script>

<a {href} class={cn('transition-colors hover:text-foreground', className)}>
  {#if children}
    {@render children()}
  {/if}
</a>
