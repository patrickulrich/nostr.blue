<script lang="ts">
  import { nip19 } from 'nostr-tools';
  import { createQuery } from '@tanstack/svelte-query';
  import { fetchAuthor, type AuthorData } from '$lib/stores/author.svelte';
  import { cn } from '$lib/utils';
  import { genUserName } from '$lib/genUserName';

  interface Props {
    pubkey: string;
  }

  let { pubkey }: Props = $props();

  const npub = nip19.npubEncode(pubkey);

  // Fetch author data using Welshman
  // @ts-expect-error - TanStack Query in Svelte requires createQuery to be called within component context
  const authorQuery = createQuery<AuthorData>(() => ({
    queryKey: ['author', pubkey],
    queryFn: ({ signal }) => fetchAuthor(pubkey, signal),
    enabled: !!pubkey,
    staleTime: 5 * 60 * 1000 // 5 minutes
  }));

  const author = $derived($authorQuery.data as AuthorData | undefined);

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
