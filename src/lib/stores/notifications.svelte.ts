import type { TrustedEvent } from '@welshman/util';

export type NotificationType = 'reply' | 'mention' | 'reaction' | 'repost' | 'zap';

export interface NotificationEvent {
	type: NotificationType;
	event: TrustedEvent;
	targetEventId?: string; // The event being replied to, reacted to, etc.
}

/**
 * Categorize notification events based on their kind and tags
 * @param events Array of Nostr events
 * @returns Array of typed notification events
 */
export function categorizeNotifications(events: TrustedEvent[]): NotificationEvent[] {
	const notifications: NotificationEvent[] = [];

	events.forEach((event) => {
		switch (event.kind) {
			case 1: {
				// Text notes - check if it's a reply or mention
				const replyTag = event.tags.find((tag) => tag[0] === 'e');
				if (replyTag) {
					notifications.push({
						type: 'reply',
						event,
						targetEventId: replyTag[1]
					});
				} else {
					notifications.push({
						type: 'mention',
						event
					});
				}
				break;
			}
			case 7: {
				// Reactions
				const eventTag = event.tags.find((tag) => tag[0] === 'e');
				const addrTag = event.tags.find((tag) => tag[0] === 'a');
				notifications.push({
					type: 'reaction',
					event,
					targetEventId: eventTag?.[1] ?? addrTag?.[1]
				});
				break;
			}
			case 6: {
				// Reposts
				const eventTag = event.tags.find((tag) => tag[0] === 'e');
				const addrTag = event.tags.find((tag) => tag[0] === 'a');
				notifications.push({
					type: 'repost',
					event,
					targetEventId: eventTag?.[1] ?? addrTag?.[1]
				});
				break;
			}
			case 9735: {
				// Zaps
				const eventTag = event.tags.find((tag) => tag[0] === 'e');
				const addrTag = event.tags.find((tag) => tag[0] === 'a');
				notifications.push({
					type: 'zap',
					event,
					targetEventId: eventTag?.[1] ?? addrTag?.[1]
				});
				break;
			}
		}
	});

	// Sort by created_at descending (newest first)
	notifications.sort((a, b) => b.event.created_at - a.event.created_at);

	return notifications;
}
