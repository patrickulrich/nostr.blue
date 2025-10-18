<script lang="ts">
  import { nip19 } from 'nostr-tools';
  import { genUserName } from '$lib/genUserName';
  import { createQuery } from '@tanstack/svelte-query';
  import { fetchAuthor, type AuthorData } from '$lib/stores/author.svelte';
  import type { CommentsData } from '$lib/stores/comments.svelte';
  import NoteContent from '$lib/components/NoteContent.svelte';
  import CommentForm from './CommentForm.svelte';
  import Comment from './Comment.svelte';
  import type { TrustedEvent } from '@welshman/util';

  interface Props {
    root: TrustedEvent | URL;
    comment: TrustedEvent;
    commentsData?: CommentsData;
    depth?: number;
    maxDepth?: number;
    limit?: number;
  }

  let { root, comment, commentsData, depth = 0, maxDepth = 3, limit }: Props = $props();

  let showReplyForm = $state(false);
  let showReplies = $state(depth < 2); // Auto-expand first 2 levels

  // Query author profile for this comment
  const authorQuery = createQuery<AuthorData>(() => ({
    queryKey: ['author', comment.pubkey],
    queryFn: ({ signal }) => fetchAuthor(comment.pubkey, signal),
    enabled: !!comment.pubkey,
    staleTime: 5 * 60 * 1000 // 5 minutes
  }));

  const author = $derived(authorQuery.data as AuthorData | undefined);

  const metadata = $derived(author?.metadata);
  const displayName = $derived(metadata?.name ?? genUserName(comment.pubkey));

  // Format time ago
  const timeAgo = $derived(() => {
    const seconds = Math.floor((Date.now() - comment.created_at * 1000) / 1000);
    if (seconds < 60) return `${seconds}s ago`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    if (days < 30) return `${days}d ago`;
    const months = Math.floor(days / 30);
    if (months < 12) return `${months}mo ago`;
    return `${Math.floor(months / 12)}y ago`;
  });

  // Get direct replies to this comment
  const replies = $derived(commentsData?.getDirectReplies?.(comment.id) || []);
  const hasReplies = $derived(replies.length > 0);
</script>

<div class="space-y-3 {depth > 0 ? 'ml-6 border-l-2 border-muted pl-4' : ''}">
  <div class="rounded-lg border bg-card/50 text-card-foreground shadow-sm">
    <div class="p-4">
      <div class="space-y-3">
        <!-- Comment Header -->
        <div class="flex items-start justify-between">
          <div class="flex items-center space-x-3">
            <a href="/{nip19.npubEncode(comment.pubkey)}">
              <div class="relative flex h-8 w-8 shrink-0 overflow-hidden rounded-full hover:ring-2 hover:ring-primary/30 transition-all cursor-pointer">
                {#if metadata?.picture}
                  <img src={metadata.picture} alt={displayName} class="aspect-square h-full w-full" />
                {:else}
                  <span class="flex h-full w-full items-center justify-center rounded-full bg-muted text-xs">
                    {displayName.charAt(0)}
                  </span>
                {/if}
              </div>
            </a>
            <div>
              <a
                href="/{nip19.npubEncode(comment.pubkey)}"
                class="font-medium text-sm hover:text-primary transition-colors"
              >
                {displayName}
              </a>
              <p class="text-xs text-muted-foreground">{timeAgo}</p>
            </div>
          </div>
        </div>

        <!-- Comment Content -->
        <div class="text-sm">
          <NoteContent event={comment} class="text-sm" />
        </div>

        <!-- Comment Actions -->
        <div class="flex items-center justify-between pt-2">
          <div class="flex items-center space-x-2">
            <button
              type="button"
              onclick={() => (showReplyForm = !showReplyForm)}
              class="inline-flex items-center justify-center rounded-md text-sm font-medium ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 hover:bg-accent hover:text-accent-foreground h-8 px-2"
            >
              <svg class="h-3 w-3 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
              </svg>
              Reply
            </button>

            {#if hasReplies}
              <button
                type="button"
                onclick={() => (showReplies = !showReplies)}
                class="inline-flex items-center justify-center rounded-md text-sm font-medium ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 hover:bg-accent hover:text-accent-foreground h-8 px-2"
              >
                {#if showReplies}
                  <svg class="h-3 w-3 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                  </svg>
                {:else}
                  <svg class="h-3 w-3 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                  </svg>
                {/if}
                {replies.length} {replies.length === 1 ? 'reply' : 'replies'}
              </button>
            {/if}
          </div>

          <!-- Comment menu -->
          <button
            type="button"
            class="inline-flex items-center justify-center rounded-md text-sm font-medium ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 hover:bg-accent hover:text-accent-foreground h-8 px-2"
            aria-label="Comment options"
          >
            <svg class="h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5v.01M12 12v.01M12 19v.01M12 6a1 1 0 110-2 1 1 0 010 2zm0 7a1 1 0 110-2 1 1 0 010 2zm0 7a1 1 0 110-2 1 1 0 010 2z" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  </div>

  <!-- Reply Form -->
  {#if showReplyForm}
    <div class="ml-6">
      <CommentForm
        {root}
        reply={comment}
        onSuccess={() => (showReplyForm = false)}
        placeholder="Write a reply..."
        compact
      />
    </div>
  {/if}

  <!-- Replies -->
  {#if hasReplies && showReplies}
    <div class="space-y-3">
      {#each replies as reply (reply.id)}
        <Comment
          {root}
          comment={reply}
          {commentsData}
          depth={depth + 1}
          {maxDepth}
          {limit}
        />
      {/each}
    </div>
  {/if}
</div>
