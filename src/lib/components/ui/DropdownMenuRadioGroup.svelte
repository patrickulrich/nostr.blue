<script lang="ts">
  import { DropdownMenu as DropdownMenuPrimitive } from 'bits-ui';

  interface Props {
    value?: string;
    onValueChange?: (value: string) => void;
    children?: import('svelte').Snippet;
  }

  let { value = $bindable(''), onValueChange, children }: Props = $props();
</script>

<DropdownMenuPrimitive.RadioGroup bind:value {onValueChange}>
  {#if children}
    {@render children()}
  {/if}
</DropdownMenuPrimitive.RadioGroup>
