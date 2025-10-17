<script lang="ts">
  import { Tabs as TabsPrimitive } from 'bits-ui';

  interface Props {
    value?: string;
    onValueChange?: (value: string) => void;
    disabled?: boolean;
    loop?: boolean;
    orientation?: 'horizontal' | 'vertical';
    children?: import('svelte').Snippet;
  }

  let {
    value = $bindable(''),
    onValueChange,
    disabled,
    loop,
    orientation,
    children
  }: Props = $props();
</script>

<TabsPrimitive.Root
  bind:value
  {onValueChange}
  {disabled}
  {loop}
  {orientation}
>
  {#if children}
    {@render children()}
  {/if}
</TabsPrimitive.Root>
