<script lang="ts">
  /* eslint-disable svelte/valid-compile */
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAttributes<HTMLSpanElement>, 'class'> {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children, ...restProps }: Props = $props();
</script>

<span class={cn('ml-auto text-xs tracking-widest text-muted-foreground', className)} {...restProps}>
  {#if children}
    {@render children()}
  {/if}
</span>
