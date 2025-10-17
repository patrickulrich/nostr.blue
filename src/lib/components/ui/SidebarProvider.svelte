<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';
  import { useIsMobile } from '$lib/utils/mobile.svelte';
  import {
    setSidebarContext,
    SIDEBAR_COOKIE_NAME,
    SIDEBAR_COOKIE_MAX_AGE,
    SIDEBAR_WIDTH,
    SIDEBAR_WIDTH_ICON,
    SIDEBAR_KEYBOARD_SHORTCUT
  } from './sidebar-utils';
  import TooltipProvider from './TooltipProvider.svelte';

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, 'class'> {
    defaultOpen?: boolean;
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    class?: string;
    style?: string;
    children?: import('svelte').Snippet;
  }

  let {
    defaultOpen = true,
    open: openProp,
    onOpenChange: setOpenProp,
    class: className,
    style,
    children,
    ...restProps
  }: Props = $props();

  const { isMobile } = useIsMobile();

  // @ts-expect-error - Svelte 5 rune scope edge case with TypeScript
  let openMobile = $state(false);
  // @ts-expect-error - Svelte 5 rune scope edge case with TypeScript
  let _open = $state(defaultOpen);
  const open = $derived(openProp !== undefined ? openProp : _open);

  function setOpen(value: boolean | ((value: boolean) => boolean)) {
    const openState = typeof value === 'function' ? value(open) : value;
    if (setOpenProp) {
      setOpenProp(openState);
    } else {
      _open = openState;
    }

    // Set cookie to keep sidebar state
    if (typeof document !== 'undefined') {
      document.cookie = `${SIDEBAR_COOKIE_NAME}=${openState}; path=/; max-age=${SIDEBAR_COOKIE_MAX_AGE}`;
    }
  }

  function setOpenMobile(value: boolean) {
    openMobile = value;
  }

  function toggleSidebar() {
    if (isMobile) {
      setOpenMobile(!openMobile);
    } else {
      setOpen(!open);
    }
  }

  $effect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === SIDEBAR_KEYBOARD_SHORTCUT && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        toggleSidebar();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  });

  const state = $derived(open ? 'expanded' : 'collapsed');

  setSidebarContext({
    get state() {
      return state;
    },
    get open() {
      return open;
    },
    setOpen,
    get openMobile() {
      return openMobile;
    },
    setOpenMobile,
    get isMobile() {
      return isMobile;
    },
    toggleSidebar
  });
</script>

<TooltipProvider delayDuration={0}>
  <div
    style:--sidebar-width={SIDEBAR_WIDTH}
    style:--sidebar-width-icon={SIDEBAR_WIDTH_ICON}
    style={style}
    class={cn(
      'group/sidebar-wrapper flex min-h-svh w-full has-[[data-variant=inset]]:bg-sidebar',
      className
    )}
    {...restProps}
  >
    {#if children}
      {@render children()}
    {/if}
  </div>
</TooltipProvider>
