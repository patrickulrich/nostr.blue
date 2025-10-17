<script lang="ts">
  import { RadioGroup as RadioGroupPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    value?: string;
    onValueChange?: (value: string) => void;
    disabled?: boolean;
    required?: boolean;
    name?: string;
    orientation?: 'horizontal' | 'vertical';
    class?: string;
    children?: import('svelte').Snippet;
  }

  let {
    value = $bindable(''),
    onValueChange,
    disabled = false,
    required = false,
    name,
    orientation = 'vertical',
    class: className,
    children
  }: Props = $props();
</script>

<RadioGroupPrimitive.Root
  bind:value
  {onValueChange}
  {disabled}
  {required}
  {name}
  {orientation}
  class={cn('grid gap-2', className)}
>
  {#if children}
    {@render children()}
  {/if}
</RadioGroupPrimitive.Root>
