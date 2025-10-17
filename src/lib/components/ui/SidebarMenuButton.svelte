<script lang="ts">
  import { cn } from '$lib/utils';
  import { sidebarMenuButtonVariants } from './sidebar-utils';
  import type { VariantProps } from 'class-variance-authority';
  import { useSidebar } from './sidebar-utils';
  import Tooltip from './Tooltip.svelte';
  import TooltipTrigger from './TooltipTrigger.svelte';
  import TooltipContent from './TooltipContent.svelte';

  interface Props extends VariantProps<typeof sidebarMenuButtonVariants> {
    isActive?: boolean;
    tooltip?: string | { children: string; [key: string]: any };
    class?: string;
    children?: import('svelte').Snippet;
  }

  let {
    isActive = false,
    variant = 'default',
    size = 'default',
    tooltip,
    class: className,
    children,
    ...restProps
  }: Props = $props();

  const { isMobile, state } = useSidebar();
</script>

{#if !tooltip}
  <button
    data-sidebar="menu-button"
    data-size={size}
    data-active={isActive}
    class={cn(sidebarMenuButtonVariants({ variant, size }), className)}
    {...restProps}
  >
    {#if children}
      {@render children()}
    {/if}
  </button>
{:else}
  {@const tooltipText = typeof tooltip === 'string' ? tooltip : tooltip.children}
  <Tooltip>
    <TooltipTrigger>
      <button
        data-sidebar="menu-button"
        data-size={size}
        data-active={isActive}
        class={cn(sidebarMenuButtonVariants({ variant, size }), className)}
        {...restProps}
      >
        {#if children}
          {@render children()}
        {/if}
      </button>
    </TooltipTrigger>
    <TooltipContent side="right" align="center" hidden={state !== 'collapsed' || isMobile}>
      {tooltipText}
    </TooltipContent>
  </Tooltip>
{/if}
