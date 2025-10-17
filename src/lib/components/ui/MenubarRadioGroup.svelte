<script lang="ts">
  import { Menubar as MenubarPrimitive } from 'bits-ui';

  interface Props {
    value?: string;
    onValueChange?: (value: string) => void;
    children?: import('svelte').Snippet;
  }

  let { value = $bindable(''), onValueChange, children }: Props = $props();
</script>

<MenubarPrimitive.RadioGroup bind:value {onValueChange}>
  {#if children}
    {@render children()}
  {/if}
</MenubarPrimitive.RadioGroup>
