<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { currentUser } from '$lib/stores/auth';
  import { fetchAuthor, type AuthorData } from '$lib/stores/author.svelte';
  import { useZaps } from '$lib/stores/zaps.svelte';
  import { getWalletStatus } from '$lib/stores/wallet.svelte';
  import type { TrustedEvent } from '@welshman/util';
  import QRCode from 'qrcode';

  interface Props {
    target: TrustedEvent;
    children?: import('svelte').Snippet;
    class?: string;
  }

  let { target, children, class: className }: Props = $props();

  let isOpen = $state(false);
  let amount = $state<number | string>(100);
  let comment = $state('Zapped with PuStack!');
  let copied = $state(false);
  let qrCodeUrl = $state('');

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
    walletStatus.activeNWC,
    () => {
      // On zap success, close dialog
      isOpen = false;
    }
  );

  // Mobile detection (simple check)
  const isMobile = $state(
    typeof window !== 'undefined' && window.innerWidth < 768
  );

  // Preset zap amounts with icons
  const presetAmounts = [
    { amount: 1, label: '✨' },
    { amount: 50, label: '⭐' },
    { amount: 100, label: '⚡' },
    { amount: 250, label: '🌟' },
    { amount: 1000, label: '🚀' },
  ];

  // Get invoice from zaps hook
  const invoice = $derived(zaps.invoice);
  const isZapping = $derived(zaps.isZapping);

  // Reset state when dialog opens/closes
  $effect(() => {
    if (isOpen) {
      amount = 100;
      zaps.resetInvoice();
      copied = false;
      qrCodeUrl = '';
      comment = 'Zapped with PuStack!';
    }
  });

  // Generate QR code when invoice changes
  $effect(() => {
    if (invoice) {
      QRCode.toDataURL(invoice.toUpperCase(), {
        width: 256,
        margin: 2,
        color: {
          dark: '#000000',
          light: '#FFFFFF'
        }
      })
        .then(url => {
          qrCodeUrl = url;
        })
        .catch(error => {
          console.error('Failed to generate QR code:', error);
          qrCodeUrl = '';
        });
    } else {
      qrCodeUrl = '';
    }
  });

  async function handleZap() {
    const finalAmount = typeof amount === 'string' ? parseInt(amount, 10) : amount;

    if (!finalAmount || finalAmount <= 0) {
      alert('Please enter a valid amount');
      return;
    }

    // Call the zap function from useZaps hook
    await zaps.zap(finalAmount, comment);
  }

  async function handleCopy() {
    if (invoice && typeof navigator !== 'undefined') {
      await navigator.clipboard.writeText(invoice);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    }
  }

  function openInWallet() {
    if (invoice) {
      const lightningUrl = `lightning:${invoice}`;
      window.open(lightningUrl, '_blank');
    }
  }

  // Don't render if conditions aren't met
  const shouldShow = $derived(
    $currentUser &&
    target &&
    $currentUser.pubkey !== target.pubkey &&
    (author?.metadata?.lud06 || author?.metadata?.lud16)
  );
</script>

{#if shouldShow}

  <!-- Desktop Dialog -->
  {#if !isMobile && isOpen}
    <div
      class="fixed inset-0 z-50 bg-black/50"
      onclick={() => (isOpen = false)}
      role="presentation"
    >
      <div
        class="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-[425px] max-h-[95vh] overflow-hidden bg-background rounded-lg shadow-lg"
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => e.stopPropagation()}
        role="dialog"
        tabindex="-1"
      >
        <div class="p-6">
          <h2 class="text-lg font-semibold mb-2">
            {invoice ? 'Lightning Payment' : 'Send a Zap'}
          </h2>
          <p class="text-sm text-muted-foreground mb-4">
            {#if invoice}
              Pay with Bitcoin Lightning Network
            {:else}
              Zaps are small Bitcoin payments that support the creator of this item.
            {/if}
          </p>

          {#if invoice}
            <!-- Invoice View -->
            <div class="space-y-4">
              <div class="text-center">
                <div class="text-2xl font-bold">{amount} sats</div>
              </div>

              <div class="h-px bg-border"></div>

              <!-- QR Code Placeholder -->
              <div class="flex justify-center">
                <div class="w-64 h-64 bg-muted rounded-lg flex items-center justify-center">
                  {#if qrCodeUrl && qrCodeUrl !== 'data:image/png;base64,placeholder'}
                    <img src={qrCodeUrl} alt="Lightning Invoice QR Code" class="w-full h-full object-contain" />
                  {:else}
                    <div class="text-muted-foreground text-sm">QR Code</div>
                  {/if}
                </div>
              </div>

              <!-- Invoice Input -->
              <div class="space-y-2">
                <label for="invoice-input" class="block text-sm font-medium">
                  Lightning Invoice
                </label>
                <div class="flex gap-2">
                  <input
                    id="invoice-input"
                    type="text"
                    value={invoice}
                    readonly
                    class="flex-1 px-3 py-2 text-xs font-mono border rounded-md bg-background"
                    onclick={(e) => e.currentTarget.select()}
                  />
                  <button
                    type="button"
                    onclick={handleCopy}
                    class="px-3 py-2 border rounded-md hover:bg-accent"
                  >
                    {#if copied}
                      ✓
                    {:else}
                      📋
                    {/if}
                  </button>
                </div>
              </div>

              <!-- Payment Buttons -->
              <div class="space-y-3">
                {#if walletStatus.webln}
                  <button
                    type="button"
                    onclick={handleZap}
                    disabled={isZapping}
                    class="w-full px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 disabled:opacity-50"
                  >
                    ⚡ {isZapping ? 'Processing...' : 'Pay with WebLN'}
                  </button>
                {/if}

                <button
                  type="button"
                  onclick={openInWallet}
                  class="w-full px-4 py-2 border rounded-md hover:bg-accent"
                >
                  🔗 Open in Lightning Wallet
                </button>

                <p class="text-xs text-muted-foreground text-center">
                  Scan the QR code or copy the invoice to pay with any Lightning wallet.
                </p>
              </div>
            </div>
          {:else}
            <!-- Zap Amount Selection -->
            <div class="space-y-4">
              <!-- Preset Amounts -->
              <div class="grid grid-cols-5 gap-2">
                {#each presetAmounts as preset}
                  <button
                    type="button"
                    onclick={() => (amount = preset.amount)}
                    class="flex flex-col items-center justify-center h-16 px-2 py-2 text-sm border rounded-md hover:bg-accent {amount === preset.amount ? 'ring-2 ring-primary' : ''}"
                  >
                    <span class="text-2xl mb-1">{preset.label}</span>
                    <span class="text-xs">{preset.amount}</span>
                  </button>
                {/each}
              </div>

              <div class="flex items-center gap-2">
                <div class="h-px flex-1 bg-border"></div>
                <span class="text-xs text-muted-foreground">OR</span>
                <div class="h-px flex-1 bg-border"></div>
              </div>

              <!-- Custom Amount -->
              <input
                type="number"
                placeholder="Custom amount"
                bind:value={amount}
                class="w-full px-3 py-2 border rounded-md bg-background"
              />

              <!-- Comment -->
              <textarea
                placeholder="Add a comment (optional)"
                bind:value={comment}
                rows="2"
                class="w-full px-3 py-2 border rounded-md bg-background resize-none"
              ></textarea>

              <!-- Zap Button -->
              <button
                type="button"
                onclick={handleZap}
                disabled={isZapping}
                class="w-full px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {#if isZapping}
                  Creating invoice...
                {:else}
                  ⚡ Zap {amount} sats
                {/if}
              </button>
            </div>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <!-- Trigger -->
  <button
    type="button"
    onclick={() => (isOpen = true)}
    class="cursor-pointer {className}"
  >
    {#if children}
      {@render children()}
    {:else}
      <span>Zap</span>
    {/if}
  </button>
{/if}
