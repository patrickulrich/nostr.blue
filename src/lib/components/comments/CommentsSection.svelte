<script lang="ts">
  import { cn } from '$lib/utils';
  import type { TrustedEvent } from '@welshman/util';
  import CommentForm from './CommentForm.svelte';
  import Comment from './Comment.svelte';

  interface Props {
    root: TrustedEvent | URL;
    title?: string;
    emptyStateMessage?: string;
    emptyStateSubtitle?: string;
    class?: string;
    limit?: number;
  }

  let {
    root,
    title = 'Comments',
    emptyStateMessage = 'No comments yet',
    emptyStateSubtitle = 'Be the first to share your thoughts!',
    class: className,
    limit = 500
  }: Props = $props();

  // TODO: Integrate with Welshman comments data
  // import { createQuery } from '@tanstack/svelte-query';
  // const commentsQuery = createQuery({
  //   queryKey: ['comments', getRootId(root), limit],
  //   queryFn: () => fetchComments(root, limit),
  // });
  // const commentsData = $derived(commentsQuery.data);
  // const isLoading = $derived(commentsQuery.isLoading);
  // const error = $derived(commentsQuery.error);

  const commentsData = $state<any>(null);
  const isLoading = $state(false);
  const error = $state<Error | null>(null);

  const comments = $derived(commentsData?.topLevelComments || []);
</script>

<div class={cn('rounded-none sm:rounded-lg mx-0 sm:mx-0 border bg-card text-card-foreground shadow-sm', className)}>
  <!-- Header -->
  <div class="px-2 pt-6 pb-4 sm:p-6">
    <h3 class="text-2xl font-semibold leading-none tracking-tight flex items-center space-x-2">
      <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
      </svg>
      <span>{title}</span>
      {#if !isLoading}
        <span class="text-sm font-normal text-muted-foreground">
          ({comments.length})
        </span>
      {/if}
    </h3>
  </div>

  <!-- Content -->
  <div class="px-2 pb-6 pt-4 sm:p-6 sm:pt-0 space-y-6">
    <!-- Comment Form -->
    <CommentForm {root} />

    <!-- Error State -->
    {#if error}
      <div class="text-center text-muted-foreground py-8">
        <svg class="h-8 w-8 mx-auto mb-2 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
        </svg>
        <p>Failed to load comments</p>
      </div>
    <!-- Loading State -->
    {:else if isLoading}
      <div class="space-y-4">
        {#each Array(3) as _, i}
          <div class="rounded-lg border bg-card/50 text-card-foreground shadow-sm">
            <div class="p-4">
              <div class="space-y-3">
                <div class="flex items-center space-x-3">
                  <div class="h-8 w-8 rounded-full bg-muted animate-pulse"></div>
                  <div class="space-y-1">
                    <div class="h-4 w-24 bg-muted rounded animate-pulse"></div>
                    <div class="h-3 w-16 bg-muted rounded animate-pulse"></div>
                  </div>
                </div>
                <div class="h-16 w-full bg-muted rounded animate-pulse"></div>
              </div>
            </div>
          </div>
        {/each}
      </div>
    <!-- Empty State -->
    {:else if comments.length === 0}
      <div class="text-center py-8 text-muted-foreground">
        <svg class="h-12 w-12 mx-auto mb-4 opacity-30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
        </svg>
        <p class="text-lg font-medium mb-2">{emptyStateMessage}</p>
        <p class="text-sm">{emptyStateSubtitle}</p>
      </div>
    <!-- Comments List -->
    {:else}
      <div class="space-y-4">
        {#each comments as comment (comment.id)}
          <Comment {root} {comment} {limit} />
        {/each}
      </div>
    {/if}
  </div>
</div>
