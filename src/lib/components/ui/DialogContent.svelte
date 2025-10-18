<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';
  import { flyAndScale } from '$lib/transitions';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<DialogPrimitive.Portal>
  <DialogPrimitive.Overlay
    class="fixed inset-0 z-50 bg-black/80 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0"
  />
  <DialogPrimitive.Content>
    {#snippet child({ props, open })}
      {#if open}
        <div
          {...props}
          class={cn(
            'fixed left-[50%] top-[50%] z-50 grid w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 border bg-background p-6 shadow-lg sm:rounded-lg',
            className
          )}
          transition:flyAndScale
        >
          {#if children}
            {@render children()}
          {/if}
          <DialogPrimitive.Close
            class="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none data-[state=open]:bg-accent data-[state=open]:text-muted-foreground"
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
          </DialogPrimitive.Close>
        </div>
      {/if}
    {/snippet}
  </DialogPrimitive.Content>
</DialogPrimitive.Portal>
