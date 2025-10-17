<script lang="ts">
  import { cn } from '$lib/utils';
  import Button from './Button.svelte';
  import { useSidebar } from './sidebar-utils';

  interface Props {
    class?: string;
    onclick?: (event: MouseEvent) => void;
    children?: import('svelte').Snippet;
  }

  let { class: className, onclick, children }: Props = $props();

  const { toggleSidebar } = useSidebar();

  function handleClick(event: MouseEvent) {
    onclick?.(event);
    toggleSidebar();
  }
</script>

<Button
  variant="ghost"
  size="icon"
  class={cn('h-7 w-7', className)}
  onclick={handleClick}
>
  {#if children}
    {@render children()}
  {:else}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      class="h-4 w-4"
    >
      <rect width="18" height="18" x="3" y="3" rx="2" />
      <path d="M9 3v18" />
    </svg>
  {/if}
  <span class="sr-only">Toggle Sidebar</span>
</Button>
