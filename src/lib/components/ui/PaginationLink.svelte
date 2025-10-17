<script lang="ts">
  import { cn } from '$lib/utils';
  import { buttonVariants } from './button-variants';
  import type { HTMLAnchorAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAnchorAttributes, 'class'> {
    isActive?: boolean;
    size?: 'default' | 'sm' | 'lg' | 'icon';
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { isActive = false, size = 'icon', class: className, children, ...restProps }: Props = $props();
</script>

<a
  aria-current={isActive ? 'page' : undefined}
  class={cn(
    buttonVariants({
      variant: isActive ? 'outline' : 'ghost',
      size
    }),
    className
  )}
  {...restProps}
>
  {#if children}
    {@render children()}
  {/if}
</a>
