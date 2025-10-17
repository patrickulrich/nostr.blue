<script lang="ts">
  import { nip19 } from 'nostr-tools';
  import { cn } from '$lib/utils';
  import { genUserName } from '$lib/genUserName';

  interface Props {
    pubkey: string;
  }

  let { pubkey }: Props = $props();

  const npub = nip19.npubEncode(pubkey);

  // TODO: Fetch author data using Welshman
  // For now, using placeholder
  let author = $state<{ metadata?: { name?: string } } | null>(null);

  // $effect(() => {
  //   // Fetch author data using Welshman load()
  //   // const data = await load({ kinds: [0], authors: [pubkey] });
  //   // author = data;
  // });

  const hasRealName = $derived(!!author?.metadata?.name);
  const displayName = $derived(author?.metadata?.name ?? genUserName(pubkey));
</script>

<a
  href="/{npub}"
  class={cn(
    "font-medium hover:underline",
    hasRealName
      ? "text-blue-500"
      : "text-gray-500 hover:text-gray-700"
  )}
>
  @{displayName}
</a>
