<script lang="ts">
  import { LinkPreview } from 'bits-ui';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<LinkPreview.Trigger class={className}>
  {#if children}
    {@render children()}
  {/if}
</LinkPreview.Trigger>
