<script lang="ts">
  interface Props {
    class?: string;
  }

  let { class: className }: Props = $props();
</script>

<div role="separator" class={className}>
  <svg
    width="15"
    height="15"
    viewBox="0 0 15 15"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    class="h-4 w-4"
  >
    <circle cx="7.5" cy="7.5" r="1.5" fill="currentColor" />
  </svg>
</div>
