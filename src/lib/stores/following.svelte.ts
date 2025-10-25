/**
 * Following/Contact List Store (Kind 3)
 *
 * Manages the user's following list with support for follow/unfollow actions.
 * Uses Welshman Router for intelligent relay selection.
 */

import { loadWithRouter } from '$lib/services/outbox';
import { publishThunk } from '@welshman/app';
import type { TrustedEvent } from '@welshman/util';
import { makeEvent } from '@welshman/util';
import { routerContext } from '@welshman/router';
import { currentPubkey } from './auth';
import { get } from 'svelte/store';

/**
 * Get the contact list event (kind 3) for a pubkey
 */
export async function getContactList(pubkey: string): Promise<TrustedEvent | null> {
	const events = await loadWithRouter({
		filters: [
			{
				kinds: [3],
				authors: [pubkey],
				limit: 1
			}
		]
	});

	return events[0] || null;
}

/**
 * Extract following pubkeys from contact list event
 */
export function extractFollowing(contactEvent: TrustedEvent | null): string[] {
	if (!contactEvent) return [];

	return contactEvent.tags
		.filter((tag) => tag[0] === 'p')
		.map((tag) => tag[1])
		.filter((pk): pk is string => typeof pk === 'string');
}

/**
 * Follow a user by adding them to the contact list
 */
export async function followUser(followPubkey: string): Promise<TrustedEvent> {
	const userPubkey = get(currentPubkey);
	if (!userPubkey) {
		throw new Error('User not logged in');
	}

	// Get current contact list
	const contactEvent = await getContactList(userPubkey);
	const existingTags = contactEvent?.tags || [];

	// Check if already following
	if (existingTags.some(([tag, pk]) => tag === 'p' && pk === followPubkey)) {
		throw new Error('Already following this user');
	}

	// Add new follow
	const newTags = [...existingTags, ['p', followPubkey]];

	// Create event
	const event = makeEvent(3, {
		content: contactEvent?.content || '',
		tags: newTags
	});

	// Get relays
	const relays = routerContext.getDefaultRelays?.() || [];

	// Publish updated contact list
	const thunk = publishThunk({ event, relays });

	// Wait for publish to complete
	await thunk.complete;

	// Get the published event from results
	const results = Object.values(thunk.results);
	const successResult = results.find((r: any) => r.status === 'success');

	if (!successResult) {
		throw new Error('Failed to publish contact list to any relay');
	}

	return (successResult as any).event;
}

/**
 * Unfollow a user by removing them from the contact list
 */
export async function unfollowUser(unfollowPubkey: string): Promise<TrustedEvent> {
	const userPubkey = get(currentPubkey);
	if (!userPubkey) {
		throw new Error('User not logged in');
	}

	// Get current contact list
	const contactEvent = await getContactList(userPubkey);
	const existingTags = contactEvent?.tags || [];

	// Remove the unfollowed pubkey
	const newTags = existingTags.filter(([tag, pk]) => !(tag === 'p' && pk === unfollowPubkey));

	// Create event
	const event = makeEvent(3, {
		content: contactEvent?.content || '',
		tags: newTags
	});

	// Get relays
	const relays = routerContext.getDefaultRelays?.() || [];

	// Publish updated contact list
	const thunk = publishThunk({ event, relays });

	// Wait for publish to complete
	await thunk.complete;

	// Get the published event from results
	const results = Object.values(thunk.results);
	const successResult = results.find((r: any) => r.status === 'success');

	if (!successResult) {
		throw new Error('Failed to publish contact list to any relay');
	}

	return (successResult as any).event;
}
