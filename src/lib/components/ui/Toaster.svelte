<script lang="ts">
  import { Toaster as Sonner } from 'svelte-sonner';

  interface Props {
    position?:
      | 'top-left'
      | 'top-center'
      | 'top-right'
      | 'bottom-left'
      | 'bottom-center'
      | 'bottom-right';
    theme?: 'light' | 'dark' | 'system';
    richColors?: boolean;
    expand?: boolean;
    duration?: number;
    closeButton?: boolean;
    class?: string;
  }

  let {
    position = 'bottom-right',
    theme = 'light',
    richColors = false,
    expand = false,
    duration = 4000,
    closeButton = false,
    class: className
  }: Props = $props();
</script>

<Sonner
  {position}
  {theme}
  {richColors}
  {expand}
  {duration}
  {closeButton}
  class={className}
  toastOptions={{
    classes: {
      toast:
        'group toast group-[.toaster]:bg-background group-[.toaster]:text-foreground group-[.toaster]:border-border group-[.toaster]:shadow-lg',
      description: 'group-[.toast]:text-muted-foreground',
      actionButton: 'group-[.toast]:bg-primary group-[.toast]:text-primary-foreground',
      cancelButton: 'group-[.toast]:bg-muted group-[.toast]:text-muted-foreground'
    }
  }}
/>