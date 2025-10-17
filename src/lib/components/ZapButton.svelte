<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { currentUser } from '$lib/stores/auth';
  import { fetchAuthor, type AuthorData } from '$lib/stores/author.svelte';
  import { useZaps } from '$lib/stores/zaps.svelte';
  import { getWalletStatus } from '$lib/stores/wallet.svelte';
  import ZapDialog from './ZapDialog.svelte';
  import type { TrustedEvent } from '@welshman/util';

  interface Props {
    target: TrustedEvent;
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

  // Query author metadata for lightning address
  // @ts-expect-error - TanStack Query in Svelte requires createQuery to be called within component context
  const authorQuery = createQuery<AuthorData>(() => ({
    queryKey: ['author', target.pubkey],
    queryFn: ({ signal }) => fetchAuthor(target.pubkey, signal),
    enabled: !!target?.pubkey,
    staleTime: 5 * 60 * 1000 // 5 minutes
  }));

  const author = $derived($authorQuery.data as AuthorData | undefined);

  // Get wallet status
  const walletStatus = getWalletStatus();

  // Use zaps hook to fetch and manage zaps
  const zaps = useZaps(
    target as import('nostr-tools').Event,
    walletStatus.webln,
    walletStatus.activeNWC
  );

  // Use external data if provided, otherwise use fetched data
  const totalSats = $derived(externalZapData?.totalSats ?? ($zaps.data?.totalSats || 0));
  const showLoading = $derived(externalZapData?.isLoading || $zaps.isLoading);

  // Don't show zap button if user is not logged in, is the author, or author has no lightning address
  const shouldShow = $derived(
    $currentUser &&
    target &&
    $currentUser.pubkey !== target.pubkey &&
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
