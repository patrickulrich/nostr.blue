<script lang="ts">
  import { Select as SelectPrimitive } from 'bits-ui';

  interface Props {
    value?: string;
    onValueChange?: (value: string) => void;
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    disabled?: boolean;
    name?: string;
    required?: boolean;
    children?: import('svelte').Snippet;
  }

  let {
    value = $bindable(''),
    onValueChange,
    open = $bindable(false),
    onOpenChange,
    disabled = false,
    name,
    required = false,
    children
  }: Props = $props();
</script>

<SelectPrimitive.Root
  type="single"
  bind:value
  {onValueChange}
  bind:open
  {onOpenChange}
  {disabled}
  {name}
  {required}
>
  {#if children}
    {@render children()}
  {/if}
</SelectPrimitive.Root>
