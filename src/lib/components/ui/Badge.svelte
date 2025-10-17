<script module lang="ts">
  import { type VariantProps } from 'class-variance-authority';
  import { badgeVariants } from './badge-variants';

  export type BadgeVariant = VariantProps<typeof badgeVariants>['variant'];
</script>

<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    variant?: BadgeVariant;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { variant = 'default', class: className, children }: Props = $props();
</script>

<div class={cn(badgeVariants({ variant }), className)}>
  {#if children}
    {@render children()}
  {/if}
</div>
