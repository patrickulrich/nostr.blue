<script lang="ts">
  import { getContext } from 'svelte';
  import { cn } from '$lib/utils';
  import Button, { type ButtonVariant } from './Button.svelte';

  interface Props {
    variant?: ButtonVariant;
    size?: 'default' | 'sm' | 'lg' | 'icon';
    class?: string;
    onclick?: () => void;
  }

  let { variant = 'outline', size = 'icon', class: className, onclick }: Props = $props();

  const { orientation, scrollNext, canScrollNext } = getContext<{
    orientation: 'horizontal' | 'vertical';
    scrollNext: () => void;
    canScrollNext: () => boolean;
  }>('carousel');

  function handleClick() {
    scrollNext();
    onclick?.();
  }
</script>

<Button
  {variant}
  {size}
  class={cn(
    'absolute h-8 w-8 rounded-full',
    orientation === 'horizontal'
      ? '-right-12 top-1/2 -translate-y-1/2'
      : '-bottom-12 left-1/2 -translate-x-1/2 rotate-90',
    className
  )}
  disabled={!canScrollNext()}
  onclick={handleClick}
>
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    class="h-4 w-4"
  >
    <path d="m9 18 6-6-6-6" />
  </svg>
  <span class="sr-only">Next slide</span>
</Button>
