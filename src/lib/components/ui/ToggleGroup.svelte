<script lang="ts">
  import { ToggleGroup as ToggleGroupPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';
  import { setContext } from 'svelte';
  import type { VariantProps } from 'class-variance-authority';
  import { toggleVariants } from './toggle-variants';

  type ToggleGroupContext = {
    variant?: VariantProps<typeof toggleVariants>['variant'];
    size?: VariantProps<typeof toggleVariants>['size'];
  };

  interface SingleProps extends VariantProps<typeof toggleVariants> {
    type: 'single';
    value?: string;
    onValueChange?: (value: string) => void;
    disabled?: boolean;
    loop?: boolean;
    rovingFocus?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  interface MultipleProps extends VariantProps<typeof toggleVariants> {
    type: 'multiple';
    value?: string[];
    onValueChange?: (value: string[]) => void;
    disabled?: boolean;
    loop?: boolean;
    rovingFocus?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  type Props = SingleProps | MultipleProps;

  let {
    type,
    value = $bindable(type === 'multiple' ? [] : ''),
    onValueChange,
    disabled,
    loop,
    rovingFocus,
    variant,
    size,
    class: className,
    children
  }: Props = $props();

  setContext<ToggleGroupContext>('toggleGroup', { variant, size });
</script>

{#if type === 'single'}
  <ToggleGroupPrimitive.Root
    type="single"
    bind:value={value as string}
    onValueChange={onValueChange as ((value: string) => void) | undefined}
    {disabled}
    {loop}
    {rovingFocus}
    class={cn('flex items-center justify-center gap-1', className)}
  >
    {#if children}
      {@render children()}
    {/if}
  </ToggleGroupPrimitive.Root>
{:else}
  <ToggleGroupPrimitive.Root
    type="multiple"
    bind:value={value as string[]}
    onValueChange={onValueChange as ((value: string[]) => void) | undefined}
    {disabled}
    {loop}
    {rovingFocus}
    class={cn('flex items-center justify-center gap-1', className)}
  >
    {#if children}
      {@render children()}
    {/if}
  </ToggleGroupPrimitive.Root>
{/if}
