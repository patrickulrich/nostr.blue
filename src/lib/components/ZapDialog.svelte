<script lang="ts">
  import { onMount } from 'svelte';

  interface Props {
    target: any; // Nostr event
    children?: import('svelte').Snippet;
    class?: string;
  }

  let { target, children, class: className }: Props = $props();

  let isOpen = $state(false);
  let amount = $state<number | string>(100);
  let comment = $state('Zapped with PuStack!');
  let invoice = $state<string | null>(null);
  let isZapping = $state(false);
  let copied = $state(false);
  let qrCodeUrl = $state('');

  // TODO: Integrate with current user hook
  // import { currentUser } from '$lib/stores/auth';
  // const user = $derived($currentUser);
  const user = $state<any>(null);

  // TODO: Integrate with author hook
  // import { createQuery } from '@tanstack/svelte-query';
  // const authorQuery = createQuery({...});
  // const author = $derived($authorQuery.data);
  const author = $state<any>(null);

  // TODO: Integrate with wallet hook
  // import { webln, activeNWC } from '$lib/stores/wallet';
  const webln = $state<any>(null);
  const activeNWC = $state<any>(null);

  // TODO: Integrate with mobile detection
  // import { isMobile } from '$lib/stores/device';
  const isMobile = $state(false);

  // Preset zap amounts with icons
  const presetAmounts = [
    { amount: 1, label: '✨' },
    { amount: 50, label: '⭐' },
    { amount: 100, label: '⚡' },
    { amount: 250, label: '🌟' },
    { amount: 1000, label: '🚀' },
  ];

  // Reset state when dialog opens/closes
  $effect(() => {
    if (isOpen) {
      amount = 100;
      invoice = null;
      copied = false;
      qrCodeUrl = '';
      comment = 'Zapped with PuStack!';
    }
  });

  // Generate QR code when invoice changes
  $effect(() => {
    if (invoice) {
      // TODO: Generate QR code using qrcode library
      // import QRCode from 'qrcode';
      // QRCode.toDataURL(invoice.toUpperCase(), {...})
      //   .then(url => qrCodeUrl = url);
      console.log('Generate QR code for invoice:', invoice);
      qrCodeUrl = 'data:image/png;base64,placeholder'; // Placeholder
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

    isZapping = true;
    try {
      // TODO: Implement zap functionality using Welshman
      // 1. Fetch author's lightning address from metadata
      // 2. Request invoice from LNURL endpoint
      // 3. Generate zap event (kind 9734)
      // 4. If WebLN is available, pay directly
      // 5. Otherwise, display invoice for manual payment

      console.log('Zapping:', { amount: finalAmount, comment, target });

      // Simulate invoice generation
      await new Promise(resolve => setTimeout(resolve, 1000));
      invoice = 'lnbc1000n1...placeholder_invoice...';

      // If WebLN is available, pay automatically
      if (webln) {
        // TODO: await webln.sendPayment(invoice);
        console.log('Paying with WebLN');
        alert('Payment sent! (placeholder)');
        isOpen = false;
      }
    } catch (error) {
      console.error('Failed to zap:', error);
      alert('Failed to create zap. Please try again.');
    } finally {
      isZapping = false;
    }
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
    user &&
    user.pubkey !== target?.pubkey &&
    (author?.metadata?.lud06 || author?.metadata?.lud16)
  );
</script>

{#if shouldShow || true}
  <!-- Always show for now until hooks are integrated -->

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
        role="dialog"
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
                {#if webln}
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
