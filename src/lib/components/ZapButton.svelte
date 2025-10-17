<script lang="ts">
  import ZapDialog from './ZapDialog.svelte';

  interface Props {
    target: any; // Nostr event
    class?: string;
    showCount?: boolean;
    zapData?: { count: number; totalSats: number; isLoading?: boolean };
  }

  let {
    target,
    class: className = "text-xs ml-1",
    showCount = true,
    zapData: externalZapData
  }: Props = $props();

  // TODO: Integrate with current user hook
  // import { currentUser } from '$lib/stores/auth';
  // const user = $derived($currentUser);
  const user = $state<any>(null);

  // TODO: Integrate with author hook to fetch author metadata
  // import { createQuery } from '@tanstack/svelte-query';
  // const authorQuery = createQuery({...});
  // const author = $derived($authorQuery.data);
  const author = $state<any>(null);

  // TODO: Integrate with wallet hook
  // import { webln, activeNWC } from '$lib/stores/wallet';
  const webln = $state<any>(null);
  const activeNWC = $state<any>(null);

  // TODO: Integrate with zaps hook to fetch zap stats
  // import { useZaps } from '$lib/hooks/useZaps';
  // const { totalSats, isLoading } = useZaps(target, webln, activeNWC);
  const fetchedTotalSats = $state(0);
  const isLoading = $state(false);

  // Use external data if provided, otherwise use fetched data
  const totalSats = $derived(externalZapData?.totalSats ?? fetchedTotalSats);
  const showLoading = $derived(externalZapData?.isLoading || isLoading);

  // Don't show zap button if user is not logged in, is the author, or author has no lightning address
  const shouldShow = $derived(
    user &&
    target &&
    user.pubkey !== target.pubkey &&
    (author?.metadata?.lud16 || author?.metadata?.lud06)
  );
</script>

{#if shouldShow}
  <ZapDialog {target}>
    <div class="flex items-center gap-1 {className}">
      <!-- Zap Icon -->
      <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M13 10V3L4 14h7v7l9-11h-7z"
        />
      </svg>
      <span class="text-xs">
        {#if showLoading}
          ...
        {:else if showCount && totalSats > 0}
          {totalSats.toLocaleString()}
        {:else}
          Zap
        {/if}
      </span>
    </div>
  </ZapDialog>
{/if}
