import {
	pubkey,
	session,
	sessions,
	signer,
	loginWithNip01,
	loginWithNip07,
	loginWithNip46,
	dropSession,
	setProfile,
	nip46Perms
} from '@welshman/app';
import { Nip07Signer, getNip07, Nip46Broker, makeSecret } from '@welshman/signer';
import { nip19 } from 'nostr-tools';
import type { Profile } from '@welshman/util';
import { derived, get } from 'svelte/store';

// Re-export Welshman stores for convenience
export { pubkey, session, sessions, signer };

// Derived stores for common use cases
export const currentUser = derived(session, ($session) => $session);
export const currentPubkey = derived(pubkey, ($pubkey) => $pubkey);
export const allSessions = derived(sessions, ($sessions) => $sessions);
export const isLoggedIn = derived(pubkey, ($pubkey) => !!$pubkey);

export const otherSessions = derived([sessions, pubkey], ([$sessions, $pubkey]) => {
	return Object.values($sessions).filter((s) => s.pubkey !== $pubkey);
});

/**
 * Check if NIP-07 browser extension is available
 */
export function hasNostrExtension(): boolean {
	return !!getNip07();
}

/**
 * Login with NIP-07 browser extension
 */
export async function loginWithExtension(): Promise<void> {
	if (!getNip07()) {
		throw new Error('No Nostr extension found. Please install a NIP-07 compatible extension.');
	}

	const signer = new Nip07Signer();
	const pubkey = await signer.getPubkey();

	loginWithNip07(pubkey);
}

/**
 * Login with nsec (private key)
 * @param nsec - The nsec string (nip19 encoded private key)
 */
export function loginWithNsec(nsec: string): void {
	const decoded = nip19.decode(nsec);

	if (decoded.type !== 'nsec') {
		throw new Error('Invalid nsec format. Please provide a valid nsec key.');
	}

	// decoded.data is Uint8Array for nsec
	const secret = decoded.data as Uint8Array;
	// Convert Uint8Array to hex string
	const secretHex = Array.from(secret)
		.map((b) => b.toString(16).padStart(2, '0'))
		.join('');
	loginWithNip01(secretHex);
}

/**
 * Login with NIP-46 bunker
 * @param bunkerUri - The bunker:// URI
 */
export async function loginWithBunker(bunkerUri: string): Promise<void> {
	let parsed;
	try {
		parsed = Nip46Broker.parseBunkerUrl(bunkerUri);
	} catch (error) {
		throw new Error('Invalid bunker URL format. Please check the URL and try again.');
	}

	const { signerPubkey, connectSecret, relays } = parsed;

	if (!signerPubkey) {
		throw new Error('Invalid bunker URL: missing signer public key.');
	}

	if (!relays || relays.length === 0) {
		throw new Error('Invalid bunker URL: no relays specified.');
	}

	// Generate ephemeral client secret for this connection
	const clientSecret = makeSecret();

	// Create broker for remote signer communication
	const broker = new Nip46Broker({
		clientSecret,
		signerPubkey,
		connectSecret,
		relays
	});

	// Connect to remote signer and verify
	const result = await broker.connect(connectSecret, nip46Perms);

	if (result !== connectSecret) {
		throw new Error('Connection verification failed. The remote signer may have rejected the request.');
	}

	// Get user's public key from remote signer
	const userPubkey = await broker.getPublicKey();

	// Create session
	loginWithNip46(userPubkey, clientSecret, signerPubkey, relays);
}

/**
 * Switch to a different logged-in account
 * @param accountPubkey - The pubkey of the account to switch to
 */
export function switchAccount(accountPubkey: string): void {
	const $sessions = get(sessions);

	if (!$sessions[accountPubkey]) {
		throw new Error('Account not found. Please log in first.');
	}

	pubkey.set(accountPubkey);
}

/**
 * Logout from an account
 * @param accountPubkey - The pubkey of the account to logout. If not provided, logs out current account.
 */
export function logout(accountPubkey?: string): void {
	const targetPubkey = accountPubkey || get(pubkey);

	if (!targetPubkey) {
		throw new Error('No account to logout from.');
	}

	dropSession(targetPubkey);
}

/**
 * Publish user profile (kind 0 event)
 * @param profile - Profile metadata to publish
 * @returns Promise that resolves when publishing is complete
 */
export async function publishProfile(profile: Profile): Promise<void> {
	const thunk = setProfile(profile);

	// Wait for all relays to complete
	await thunk.complete;

	// Check if any relay succeeded
	// The thunk itself is a Svelte store that contains the results
	const hasSuccess = Object.values(thunk.results).some(
		(r: any) => r.status === 'success'
	);

	if (!hasSuccess) {
		throw new Error('Failed to publish profile to any relay. Please try again.');
	}
}

/**
 * Validate nsec format
 * @param nsec - The nsec string to validate
 * @returns true if valid, false otherwise
 */
export function validateNsec(nsec: string): boolean {
	try {
		const decoded = nip19.decode(nsec);
		return decoded.type === 'nsec';
	} catch {
		return false;
	}
}

/**
 * Validate bunker URI format
 * @param uri - The bunker URI to validate
 * @returns true if valid, false otherwise
 */
export function validateBunkerUri(uri: string): boolean {
	try {
		const parsed = Nip46Broker.parseBunkerUrl(uri);
		return !!parsed.signerPubkey && parsed.relays.length > 0;
	} catch {
		return false;
	}
}
