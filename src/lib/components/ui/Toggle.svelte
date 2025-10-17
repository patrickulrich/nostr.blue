<script lang="ts">
  import { Toggle as TogglePrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';
  import { toggleVariants } from './toggle-variants';
  import type { VariantProps } from 'class-variance-authority';

  interface Props extends VariantProps<typeof toggleVariants> {
    pressed?: boolean;
    onPressedChange?: (pressed: boolean) => void;
    disabled?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let {
    pressed = $bindable(false),
    onPressedChange,
    disabled,
    variant,
    size,
    class: className,
    children
  }: Props = $props();
</script>

<TogglePrimitive.Root
  bind:pressed
  {onPressedChange}
  {disabled}
  class={cn(toggleVariants({ variant, size, className }))}
>
  {#if children}
    {@render children()}
  {/if}
</TogglePrimitive.Root>
