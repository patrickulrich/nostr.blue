<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';
  import { useSidebar, SIDEBAR_WIDTH_MOBILE } from './sidebar-utils';
  import Sheet from './Sheet.svelte';
  import SheetContent from './SheetContent.svelte';

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, 'class'> {
    side?: 'left' | 'right';
    variant?: 'sidebar' | 'floating' | 'inset';
    collapsible?: 'offcanvas' | 'icon' | 'none';
    class?: string;
    children?: import('svelte').Snippet;
  }

  let {
    side = 'left',
    variant = 'sidebar',
    collapsible = 'offcanvas',
    class: className,
    children,
    ...restProps
  }: Props = $props();

  const context = useSidebar();
  const { isMobile, state, openMobile, setOpenMobile } = context;
</script>

{#if collapsible === 'none'}
  <div
    class={cn(
      'flex h-full w-[--sidebar-width] flex-col bg-sidebar text-sidebar-foreground',
      className
    )}
    {...restProps}
  >
    {#if children}
      {@render children()}
    {/if}
  </div>
{:else if isMobile}
  <Sheet open={openMobile} onOpenChange={setOpenMobile}>
    <SheetContent
      class="w-[--sidebar-width] bg-sidebar p-0 text-sidebar-foreground [&>button]:hidden"
      style="--sidebar-width: {SIDEBAR_WIDTH_MOBILE}"
      {side}
    >
      <div class="flex h-full w-full flex-col" data-sidebar="sidebar" data-mobile="true">
        {#if children}
          {@render children()}
        {/if}
      </div>
    </SheetContent>
  </Sheet>
{:else}
  <div
    class="group peer hidden md:block text-sidebar-foreground"
    data-state={state}
    data-collapsible={state === 'collapsed' ? collapsible : ''}
    data-variant={variant}
    data-side={side}
  >
    <!-- This is what handles the sidebar gap on desktop -->
    <div
      class={cn(
        'duration-200 relative h-svh w-[--sidebar-width] bg-transparent transition-[width] ease-linear',
        'group-data-[collapsible=offcanvas]:w-0',
        'group-data-[side=right]:rotate-180',
        variant === 'floating' || variant === 'inset'
          ? 'group-data-[collapsible=icon]:w-[calc(var(--sidebar-width-icon)_+_theme(spacing.4))]'
          : 'group-data-[collapsible=icon]:w-[--sidebar-width-icon]'
      )}
    ></div>
    <div
      class={cn(
        'duration-200 fixed inset-y-0 z-10 hidden h-svh w-[--sidebar-width] transition-[left,right,width] ease-linear md:flex',
        side === 'left'
          ? 'left-0 group-data-[collapsible=offcanvas]:left-[calc(var(--sidebar-width)*-1)]'
          : 'right-0 group-data-[collapsible=offcanvas]:right-[calc(var(--sidebar-width)*-1)]',
        variant === 'floating' || variant === 'inset'
          ? 'p-2 group-data-[collapsible=icon]:w-[calc(var(--sidebar-width-icon)_+_theme(spacing.4)_+2px)]'
          : 'group-data-[collapsible=icon]:w-[--sidebar-width-icon] group-data-[side=left]:border-r group-data-[side=right]:border-l',
        className
      )}
      {...restProps}
    >
      <div
        data-sidebar="sidebar"
        class="flex h-full w-full flex-col bg-sidebar group-data-[variant=floating]:rounded-lg group-data-[variant=floating]:border group-data-[variant=floating]:border-sidebar-border group-data-[variant=floating]:shadow"
      >
        {#if children}
          {@render children()}
        {/if}
      </div>
    </div>
  </div>
{/if}
