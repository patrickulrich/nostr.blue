<script lang="ts">
  import { Dialog as SheetPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';
  import { sheetVariants } from './sheet-variants';
  import type { VariantProps } from 'class-variance-authority';

  interface Props extends VariantProps<typeof sheetVariants> {
    class?: string;
    style?: string;
    children?: import('svelte').Snippet;
  }

  let { side = 'right', class: className, style, children }: Props = $props();
</script>

<SheetPrimitive.Portal>
  <SheetPrimitive.Overlay
    class="fixed inset-0 z-50 bg-black/80 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0"
  />
  <SheetPrimitive.Content class={cn(sheetVariants({ side }), className)} {style}>
    {#if children}
      {@render children()}
    {/if}
    <SheetPrimitive.Close
      class="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none data-[state=open]:bg-secondary"
    >
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
        <path d="M18 6 6 18" />
        <path d="m6 6 12 12" />
      </svg>
      <span class="sr-only">Close</span>
    </SheetPrimitive.Close>
  </SheetPrimitive.Content>
</SheetPrimitive.Portal>
