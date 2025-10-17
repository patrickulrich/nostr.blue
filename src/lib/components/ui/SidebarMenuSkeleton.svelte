<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';
  import Skeleton from './Skeleton.svelte';

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, 'class'> {
    showIcon?: boolean;
    class?: string;
  }

  let { showIcon = false, class: className, ...restProps }: Props = $props();

  // Random width between 50 to 90%
  const width = $derived(`${Math.floor(Math.random() * 40) + 50}%`);
</script>

<div
  data-sidebar="menu-skeleton"
  class={cn('rounded-md h-8 flex gap-2 px-2 items-center', className)}
  {...restProps}
>
  {#if showIcon}
    <Skeleton class="size-4 rounded-md" data-sidebar="menu-skeleton-icon" />
  {/if}
  <Skeleton
    class="h-4 flex-1 max-w-[--skeleton-width]"
    data-sidebar="menu-skeleton-text"
    style="--skeleton-width: {width}"
  />
</div>
