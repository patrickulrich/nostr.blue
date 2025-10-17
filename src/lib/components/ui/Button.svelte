<script module lang="ts">
  import { type VariantProps } from 'class-variance-authority';
  import { buttonVariants } from './button-variants';

  export type ButtonVariant = VariantProps<typeof buttonVariants>['variant'];
  export type ButtonSize = VariantProps<typeof buttonVariants>['size'];
</script>

<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    variant?: ButtonVariant;
    size?: ButtonSize;
    type?: 'button' | 'submit' | 'reset';
    disabled?: boolean;
    class?: string;
    onclick?: (e: MouseEvent) => void;
    children?: import('svelte').Snippet;
  }

  let {
    variant = 'default',
    size = 'default',
    type = 'button',
    disabled = false,
    class: className,
    onclick,
    children
  }: Props = $props();
</script>

<button
  {type}
  {disabled}
  class={cn(buttonVariants({ variant, size }), className)}
  {onclick}
>
  {#if children}
    {@render children()}
  {/if}
</button>
