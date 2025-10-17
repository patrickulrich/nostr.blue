import type { TrustedEvent } from '@welshman/util';
import { COMMENT, isParameterizedReplaceableKind, isPlainReplaceableKind } from '@welshman/util';
import { createMutation, useQueryClient } from '@tanstack/svelte-query';
import { publishNostrEvent } from './publish.svelte';

interface PostCommentParams {
	/** The root event to comment on */
	root: TrustedEvent | URL;
	/** Optional reply to another comment */
	reply?: TrustedEvent | URL;
	/** Comment content */
	content: string;
}

/**
 * TanStack mutation for posting NIP-22 comments
 * Automatically builds the correct tags for the comment based on root and reply
 *
 * @returns TanStack mutation for posting comments
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { usePostComment } from '$lib/stores/postComment.svelte';
 *
 *   let { event } = $props();
 *   const postComment = usePostComment();
 *
 *   let content = $state('');
 *
 *   function handleSubmit() {
 *     $postComment.mutate({
 *       root: event,
 *       content
 *     });
 *   }
 * </script>
 *
 * <textarea bind:value={content} />
 * <button onclick={handleSubmit} disabled={$postComment.isPending}>
 *   {$postComment.isPending ? 'Posting...' : 'Post Comment'}
 * </button>
 * ```
 */
export function usePostComment() {
	const queryClient = useQueryClient();

	return createMutation({
		mutationFn: async ({ root, reply, content }: PostCommentParams) => {
			const tags: string[][] = [];

			// d-tag identifiers
			const dRoot =
				root instanceof URL ? '' : root.tags.find(([name]) => name === 'd')?.[1] ?? '';
			const dReply =
				reply instanceof URL ? '' : reply?.tags.find(([name]) => name === 'd')?.[1] ?? '';

			// Root event tags (uppercase)
			if (root instanceof URL) {
				tags.push(['I', root.toString()]);
			} else if (isParameterizedReplaceableKind(root.kind)) {
				tags.push(['A', `${root.kind}:${root.pubkey}:${dRoot}`]);
			} else if (isPlainReplaceableKind(root.kind)) {
				tags.push(['A', `${root.kind}:${root.pubkey}:`]);
			} else {
				tags.push(['E', root.id]);
			}
			if (root instanceof URL) {
				tags.push(['K', root.hostname]);
			} else {
				tags.push(['K', root.kind.toString()]);
				tags.push(['P', root.pubkey]);
			}

			// Reply event tags (lowercase)
			if (reply) {
				if (reply instanceof URL) {
					tags.push(['i', reply.toString()]);
				} else if (isParameterizedReplaceableKind(reply.kind)) {
					tags.push(['a', `${reply.kind}:${reply.pubkey}:${dReply}`]);
				} else if (isPlainReplaceableKind(reply.kind)) {
					tags.push(['a', `${reply.kind}:${reply.pubkey}:`]);
				} else {
					tags.push(['e', reply.id]);
				}
				if (reply instanceof URL) {
					tags.push(['k', reply.hostname]);
				} else {
					tags.push(['k', reply.kind.toString()]);
					tags.push(['p', reply.pubkey]);
				}
			} else {
				// If this is a top-level comment, use the root event's tags (lowercase)
				if (root instanceof URL) {
					tags.push(['i', root.toString()]);
				} else if (isParameterizedReplaceableKind(root.kind)) {
					tags.push(['a', `${root.kind}:${root.pubkey}:${dRoot}`]);
				} else if (isPlainReplaceableKind(root.kind)) {
					tags.push(['a', `${root.kind}:${root.pubkey}:`]);
				} else {
					tags.push(['e', root.id]);
				}
				if (root instanceof URL) {
					tags.push(['k', root.hostname]);
				} else {
					tags.push(['k', root.kind.toString()]);
					tags.push(['p', root.pubkey]);
				}
			}

			const event = await publishNostrEvent({
				kind: COMMENT,
				content,
				tags
			});

			return event;
		},
		onSuccess: (_, { root }) => {
			// Invalidate and refetch comments
			queryClient.invalidateQueries({
				queryKey: ['comments', root instanceof URL ? root.toString() : root.id]
			});
		}
	});
}
