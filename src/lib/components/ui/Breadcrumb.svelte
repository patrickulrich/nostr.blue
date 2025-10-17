<script lang="ts">
  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<nav aria-label="breadcrumb" class={className}>
  {#if children}
    {@render children()}
  {/if}
</nav>
