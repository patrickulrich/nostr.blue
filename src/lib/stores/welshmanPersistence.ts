/**
 * Welshman State Persistence
 *
 * Welshman stores (pubkey, sessions) are not persisted by default.
 * This module sets up localStorage persistence for authentication state.
 */

import { browser } from '$app/environment';
import { pubkey, sessions } from '@welshman/app';
import { get } from 'svelte/store';

const STORAGE_KEY_PUBKEY = 'welshman:pubkey';
const STORAGE_KEY_SESSIONS = 'welshman:sessions';

/**
 * Load persisted state from localStorage on app initialization
 */
export function loadPersistedState() {
	if (!browser) return;

	try {
		// Load pubkey
		const storedPubkey = localStorage.getItem(STORAGE_KEY_PUBKEY);
		if (storedPubkey && storedPubkey !== 'undefined' && storedPubkey !== 'null') {
			const parsedPubkey = JSON.parse(storedPubkey);
			if (parsedPubkey) {
				pubkey.set(parsedPubkey);
			}
		}

		// Load sessions
		const storedSessions = localStorage.getItem(STORAGE_KEY_SESSIONS);
		if (storedSessions) {
			const parsedSessions = JSON.parse(storedSessions);
			if (parsedSessions && typeof parsedSessions === 'object') {
				sessions.set(parsedSessions);
			}
		}

		console.log('Welshman state loaded from localStorage');
	} catch (error) {
		console.error('Failed to load Welshman state from localStorage:', error);
		// Clear corrupted data
		localStorage.removeItem(STORAGE_KEY_PUBKEY);
		localStorage.removeItem(STORAGE_KEY_SESSIONS);
	}
}

/**
 * Set up automatic persistence for Welshman stores
 * Call this once during app initialization
 */
export function setupWelshmanPersistence() {
	if (!browser) return () => {};

	// Load persisted state first
	loadPersistedState();

	// Subscribe to pubkey changes and persist
	const unsubPubkey = pubkey.subscribe((value) => {
		try {
			if (value === undefined || value === null) {
				localStorage.removeItem(STORAGE_KEY_PUBKEY);
			} else {
				localStorage.setItem(STORAGE_KEY_PUBKEY, JSON.stringify(value));
			}
		} catch (error) {
			console.error('Failed to persist pubkey:', error);
		}
	});

	// Subscribe to sessions changes and persist
	const unsubSessions = sessions.subscribe((value) => {
		try {
			if (!value || Object.keys(value).length === 0) {
				localStorage.removeItem(STORAGE_KEY_SESSIONS);
			} else {
				localStorage.setItem(STORAGE_KEY_SESSIONS, JSON.stringify(value));
			}
		} catch (error) {
			console.error('Failed to persist sessions:', error);
		}
	});

	console.log('Welshman persistence enabled');

	// Return cleanup function
	return () => {
		unsubPubkey();
		unsubSessions();
	};
}

/**
 * Clear all persisted Welshman state
 * Useful for complete logout
 */
export function clearPersistedState() {
	if (!browser) return;

	try {
		localStorage.removeItem(STORAGE_KEY_PUBKEY);
		localStorage.removeItem(STORAGE_KEY_SESSIONS);
		console.log('Welshman persisted state cleared');
	} catch (error) {
		console.error('Failed to clear persisted state:', error);
	}
}
