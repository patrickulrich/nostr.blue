<script lang="ts">
  import { Command as CommandPrimitive } from 'cmdk-sv';
  import Dialog from './Dialog.svelte';
  import DialogContent from './DialogContent.svelte';

  interface Props {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    children?: import('svelte').Snippet;
  }

  let { open = $bindable(false), onOpenChange, children }: Props = $props();
</script>

<Dialog bind:open {onOpenChange}>
  <DialogContent class="overflow-hidden p-0 shadow-lg">
    <CommandPrimitive.Root
      class="[&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-muted-foreground [&_[cmdk-group]:not([hidden])_~[cmdk-group]]:pt-0 [&_[cmdk-group]]:px-2 [&_[cmdk-input-wrapper]_svg]:h-5 [&_[cmdk-input-wrapper]_svg]:w-5 [&_[cmdk-input]]:h-12 [&_[cmdk-item]]:px-2 [&_[cmdk-item]]:py-3 [&_[cmdk-item]_svg]:h-5 [&_[cmdk-item]_svg]:w-5"
    >
      {#if children}
        {@render children()}
      {/if}
    </CommandPrimitive.Root>
  </DialogContent>
</Dialog>
