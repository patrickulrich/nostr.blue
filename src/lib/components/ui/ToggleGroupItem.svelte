<script lang="ts">
  import { ToggleGroup as ToggleGroupPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';
  import { getContext } from 'svelte';
  import { toggleVariants } from './toggle-variants';
  import type { VariantProps } from 'class-variance-authority';

  type ToggleGroupContext = {
    variant?: VariantProps<typeof toggleVariants>['variant'];
    size?: VariantProps<typeof toggleVariants>['size'];
  };

  interface Props extends VariantProps<typeof toggleVariants> {
    value: string;
    disabled?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { value, disabled, variant, size, class: className, children }: Props = $props();

  const context = getContext<ToggleGroupContext>('toggleGroup');
</script>

<ToggleGroupPrimitive.Item
  {value}
  {disabled}
  class={cn(
    toggleVariants({
      variant: context?.variant || variant,
      size: context?.size || size
    }),
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
</ToggleGroupPrimitive.Item>
