<script lang="ts">
  import { currentUser } from '$lib/stores/auth';
  import LoginArea from '$lib/components/auth/LoginArea.svelte';
  import type { TrustedEvent } from '@welshman/util';

  interface Props {
    root: TrustedEvent | URL;
    reply?: TrustedEvent | URL;
    onSuccess?: () => void;
    placeholder?: string;
    compact?: boolean;
  }

  let { root, reply, onSuccess, placeholder = 'Write a comment...', compact = false }: Props = $props();

  let content = $state('');
  let isPending = $state(false);

  // TODO: Integrate with Welshman publishing
  // import { publish } from '@welshman/app';

  async function handleSubmit(e: Event) {
    e.preventDefault();

    if (!content.trim() || !$currentUser) return;

    isPending = true;
    try {
      // TODO: Implement comment posting using Welshman
      // const tags: string[][] = [];
      //
      // // Add root reference
      // if (root instanceof URL) {
      //   tags.push(['r', root.toString()]);
      // } else {
      //   tags.push(['e', root.id, '', 'root']);
      // }
      //
      // // Add reply reference
      // if (reply) {
      //   if (reply instanceof URL) {
      //     tags.push(['r', reply.toString()]);
      //   } else {
      //     tags.push(['e', reply.id, '', 'reply']);
      //     tags.push(['p', reply.pubkey]);
      //   }
      // }
      //
      // const thunk = publish({
      //   kind: 1,
      //   content: content.trim(),
      //   tags,
      // });
      //
      // await thunk.complete;

      console.log('Post comment:', { content: content.trim(), root, reply });

      content = '';
      onSuccess?.();
    } catch (error) {
      console.error('Failed to post comment:', error);
      alert('Failed to post comment. Please try again.');
    } finally {
      isPending = false;
    }
  }
</script>

<div class="rounded-lg border bg-card text-card-foreground shadow-sm {compact ? 'border-dashed' : ''}">
  <div class="{compact ? 'p-4' : 'p-6'}">
    {#if !$currentUser}
      <div class="text-center space-y-4">
        <div class="flex items-center justify-center space-x-2 text-muted-foreground">
          <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
          </svg>
          <span>Sign in to {reply ? 'reply' : 'comment'}</span>
        </div>
        <LoginArea />
      </div>
    {:else}
      <form onsubmit={handleSubmit} class="space-y-4">
        <textarea
          bind:value={content}
          {placeholder}
          class="flex min-h-[80px] {compact ? 'min-h-[80px]' : 'min-h-[100px]'} w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 resize-none"
          disabled={isPending}
        ></textarea>
        <div class="flex justify-between items-center">
          <span class="text-sm text-muted-foreground">
            {reply ? 'Replying to comment' : 'Adding to the discussion'}
          </span>
          <button
            type="submit"
            disabled={!content.trim() || isPending}
            class="inline-flex items-center justify-center rounded-md text-sm font-medium ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 bg-primary text-primary-foreground hover:bg-primary/90 {compact ? 'h-9 px-3' : 'h-10 px-4 py-2'}"
          >
            <svg class="h-4 w-4 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
            </svg>
            {isPending ? 'Posting...' : (reply ? 'Reply' : 'Comment')}
          </button>
        </div>
      </form>
    {/if}
  </div>
</div>
