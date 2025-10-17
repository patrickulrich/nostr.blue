import type { TrustedEvent, Filter } from '@welshman/util';
import { COMMENT, isParameterizedReplaceableKind, isPlainReplaceableKind } from '@welshman/util';
import { createQuery } from '@tanstack/svelte-query';
import { load } from '@welshman/net';

/**
 * Get the value of a tag from an event
 */
function getTagValue(event: TrustedEvent, tagName: string): string | undefined {
	const tag = event.tags.find(([name]) => name === tagName);
	return tag?.[1];
}

/**
 * Query NIP-22 comments for a Nostr event or URL
 * Returns all comments with utilities to filter by depth and hierarchy
 *
 * @param root - The root event or URL to fetch comments for
 * @param limit - Optional limit on the number of comments to fetch
 * @returns TanStack Query with comment data and helper functions
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { useComments } from '$lib/stores/comments.svelte';
 *
 *   let { event } = $props();
 *   const comments = useComments(event);
 *
 *   let topLevel = $derived($comments.data?.topLevelComments ?? []);
 * </script>
 *
 * {#if $comments.isLoading}
 *   <div>Loading comments...</div>
 * {:else if topLevel.length > 0}
 *   {#each topLevel as comment}
 *     <Comment {comment} />
 *   {/each}
 * {/if}
 * ```
 */
export function useComments(root: TrustedEvent | URL | undefined, limit?: number) {
	return createQuery(() => ({
		queryKey: ['comments', root instanceof URL ? root.toString() : root?.id, limit],
		queryFn: async ({ signal }) => {
			if (!root) {
				return {
					allComments: [],
					topLevelComments: [],
					getDescendants: () => [],
					getDirectReplies: () => []
				};
			}

			const filter: Filter = { kinds: [COMMENT] };

			if (root instanceof URL) {
				filter['#I'] = [root.toString()];
			} else if (isParameterizedReplaceableKind(root.kind)) {
				const d = root.tags.find(([name]) => name === 'd')?.[1] ?? '';
				filter['#A'] = [`${root.kind}:${root.pubkey}:${d}`];
			} else if (isPlainReplaceableKind(root.kind)) {
				filter['#A'] = [`${root.kind}:${root.pubkey}:`];
			} else {
				filter['#E'] = [root.id];
			}

			if (typeof limit === 'number') {
				filter.limit = limit;
			}

			// Query for all kind 1111 comments
			const events = await load({
				relays: [],
				filters: [filter],
				signal,
				timeout: 5000
			});

			// Filter top-level comments
			const topLevelComments = events.filter((comment) => {
				if (root instanceof URL) {
					return getTagValue(comment, 'i') === root.toString();
				} else if (isParameterizedReplaceableKind(root.kind)) {
					const d = getTagValue(root, 'd') ?? '';
					return getTagValue(comment, 'a') === `${root.kind}:${root.pubkey}:${d}`;
				} else if (isPlainReplaceableKind(root.kind)) {
					return getTagValue(comment, 'a') === `${root.kind}:${root.pubkey}:`;
				} else {
					return getTagValue(comment, 'e') === root.id;
				}
			});

			// Helper function to get all descendants of a comment
			const getDescendants = (parentId: string): TrustedEvent[] => {
				const directReplies = events.filter((comment) => {
					const eTag = getTagValue(comment, 'e');
					return eTag === parentId;
				});

				const allDescendants = [...directReplies];

				// Recursively get descendants of each direct reply
				for (const reply of directReplies) {
					allDescendants.push(...getDescendants(reply.id));
				}

				return allDescendants;
			};

			// Create a map of comment ID to its descendants
			const commentDescendants = new Map<string, TrustedEvent[]>();
			for (const comment of events) {
				commentDescendants.set(comment.id, getDescendants(comment.id));
			}

			// Sort top-level comments by creation time (newest first)
			const sortedTopLevel = topLevelComments.sort((a, b) => b.created_at - a.created_at);

			return {
				allComments: events,
				topLevelComments: sortedTopLevel,
				getDescendants: (commentId: string) => {
					const descendants = commentDescendants.get(commentId) || [];
					// Sort descendants by creation time (oldest first for threaded display)
					return descendants.sort((a, b) => a.created_at - b.created_at);
				},
				getDirectReplies: (commentId: string) => {
					const directReplies = events.filter((comment) => {
						const eTag = getTagValue(comment, 'e');
						return eTag === commentId;
					});
					// Sort direct replies by creation time (oldest first for threaded display)
					return directReplies.sort((a, b) => a.created_at - b.created_at);
				}
			};
		},
		enabled: !!root
	}));
}

export type CommentsData = {
	allComments: TrustedEvent[];
	topLevelComments: TrustedEvent[];
	getDescendants: (commentId: string) => TrustedEvent[];
	getDirectReplies: (commentId: string) => TrustedEvent[];
};
