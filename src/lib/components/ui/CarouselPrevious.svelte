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

  const { orientation, scrollPrev, canScrollPrev } = getContext<{
    orientation: 'horizontal' | 'vertical';
    scrollPrev: () => void;
    canScrollPrev: () => boolean;
  }>('carousel');

  function handleClick() {
    scrollPrev();
    onclick?.();
  }
</script>

<Button
  {variant}
  {size}
  class={cn(
    'absolute h-8 w-8 rounded-full',
    orientation === 'horizontal'
      ? '-left-12 top-1/2 -translate-y-1/2'
      : '-top-12 left-1/2 -translate-x-1/2 rotate-90',
    className
  )}
  disabled={!canScrollPrev()}
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
    <path d="m15 18-6-6 6-6" />
  </svg>
  <span class="sr-only">Previous slide</span>
</Button>
