<script lang="ts">
  import { Tooltip as TooltipPrimitive } from 'bits-ui';

  interface Props {
    delayDuration?: number;
    skipDelayDuration?: number;
    disableHoverableContent?: boolean;
    disableCloseOnTriggerClick?: boolean;
    disabled?: boolean;
    ignoreNonKeyboardFocus?: boolean;
    children?: import('svelte').Snippet;
  }

  let {
    delayDuration,
    skipDelayDuration,
    disableHoverableContent,
    disableCloseOnTriggerClick,
    disabled,
    ignoreNonKeyboardFocus,
    children
  }: Props = $props();
</script>

<TooltipPrimitive.Provider
  {delayDuration}
  {skipDelayDuration}
  {disableHoverableContent}
  {disableCloseOnTriggerClick}
  {disabled}
  {ignoreNonKeyboardFocus}
>
  {#if children}
    {@render children()}
  {/if}
</TooltipPrimitive.Provider>
