<script lang="ts">
  import { nip19 } from 'nostr-tools';
  import { cn } from '$lib/utils';
  import NostrMention from './NostrMention.svelte';

  interface Props {
    event: { content: string; [key: string]: any };
    class?: string;
  }

  let { event, class: className }: Props = $props();

  // Parse content to find URLs, Nostr references, and hashtags
  const parsedContent = $derived(() => {
    const text = event.content;

    // Regex to find URLs, Nostr references, and hashtags
    const regex = /(https?:\/\/[^\s]+)|nostr:(npub1|note1|nprofile1|nevent1|naddr1)([023456789acdefghjklmnpqrstuvwxyz]+)|(#\w+)/g;

    const parts: Array<{ type: string; content: string; data?: any }> = [];
    let lastIndex = 0;
    let match: RegExpExecArray | null;

    while ((match = regex.exec(text)) !== null) {
      const [fullMatch, url, nostrPrefix, nostrData, hashtag] = match;
      const index = match.index;

      // Add text before this match
      if (index > lastIndex) {
        parts.push({
          type: 'text',
          content: text.substring(lastIndex, index)
        });
      }

      if (url) {
        // Handle URLs
        parts.push({
          type: 'url',
          content: url
        });
      } else if (nostrPrefix && nostrData) {
        // Handle Nostr references
        try {
          const nostrId = `${nostrPrefix}${nostrData}`;
          const decoded = nip19.decode(nostrId);

          if (decoded.type === 'npub') {
            const pubkey = decoded.data as string;
            parts.push({
              type: 'mention',
              content: fullMatch,
              data: { pubkey }
            });
          } else {
            // For other types, just show as a link
            parts.push({
              type: 'nostr-link',
              content: fullMatch,
              data: { nostrId }
            });
          }
        } catch {
          // If decoding fails, just render as text
          parts.push({
            type: 'text',
            content: fullMatch
          });
        }
      } else if (hashtag) {
        // Handle hashtags
        const tag = hashtag.slice(1); // Remove the #
        parts.push({
          type: 'hashtag',
          content: hashtag,
          data: { tag }
        });
      }

      lastIndex = index + fullMatch.length;
    }

    // Add any remaining text
    if (lastIndex < text.length) {
      parts.push({
        type: 'text',
        content: text.substring(lastIndex)
      });
    }

    // If no special content was found, return plain text
    if (parts.length === 0) {
      return [{ type: 'text', content: text }];
    }

    return parts;
  });
</script>

<div class={cn("whitespace-pre-wrap break-words", className)}>
  {#each parsedContent() as part, i (i)}
    {#if part.type === 'text'}
      {part.content}
    {:else if part.type === 'url'}
      <a
        href={part.content}
        target="_blank"
        rel="noopener noreferrer"
        class="text-blue-500 hover:underline"
      >
        {part.content}
      </a>
    {:else if part.type === 'mention'}
      <NostrMention pubkey={part.data.pubkey} />
    {:else if part.type === 'nostr-link'}
      <a
        href="/{part.data.nostrId}"
        class="text-blue-500 hover:underline"
      >
        {part.content}
      </a>
    {:else if part.type === 'hashtag'}
      <a
        href="/t/{part.data.tag}"
        class="text-blue-500 hover:underline"
      >
        {part.content}
      </a>
    {/if}
  {/each}
</div>
