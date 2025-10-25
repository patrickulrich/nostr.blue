import type { TrustedEvent } from '@welshman/util';

/**
 * Extended event type for reposts (kind 6)
 * Includes metadata about the original event and repost author
 */
export interface RepostEvent extends TrustedEvent {
	_repostedEvent?: TrustedEvent | null;
	_repostAuthor?: string;
}

/**
 * Runtime type guard to validate TrustedEvent structure
 * @param obj - Object to validate
 * @returns True if obj is a valid TrustedEvent
 */
export function isTrustedEvent(obj: unknown): obj is TrustedEvent {
	if (!obj || typeof obj !== 'object') return false;

	const event = obj as Record<string, unknown>;

	return (
		typeof event.id === 'string' &&
		typeof event.pubkey === 'string' &&
		typeof event.created_at === 'number' &&
		typeof event.kind === 'number' &&
		typeof event.content === 'string' &&
		Array.isArray(event.tags)
	);
}
