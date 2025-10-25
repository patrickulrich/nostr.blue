/**
 * Nostr.Band API integration for trending content
 * API Docs: https://api.nostr.band
 */

const NOSTR_BAND_API = 'https://api.nostr.band';

export interface TrendingNote {
	event: {
		id: string;
		pubkey: string;
		created_at: number;
		kind: number;
		tags: string[][];
		content: string;
		sig: string;
	};
	author: {
		id?: string;
		pubkey: string;
		content: string; // JSON string containing profile data
	};
	profile?: {
		name?: string;
		display_name?: string;
		picture?: string;
		nip05?: string;
		about?: string;
	};
	stats?: {
		replies?: number;
		reactions?: number;
		reposts?: number;
		zaps?: number;
	};
}

interface NostrBandApiNote {
	event: {
		id: string;
		pubkey: string;
		created_at: number;
		kind: number;
		tags: string[][];
		content: string;
		sig: string;
	};
	author: {
		id?: string;
		pubkey: string;
		content: string;
	};
	stats?: {
		replies_count?: number;
		reactions_count?: number;
		reposts_count?: number;
		zaps_msats?: number;
	};
}

interface NostrBandResponse {
	notes: NostrBandApiNote[];
}

/**
 * Fetch trending notes from Nostr.Band
 * Returns the top trending posts in the last 24 hours
 * @param limit Number of notes to fetch (default 10)
 */
export async function getTrendingNotes(limit: number = 10): Promise<TrendingNote[]> {
	try {
		const response = await fetch(`${NOSTR_BAND_API}/v0/trending/notes`);
		if (!response.ok) {
			throw new Error(`Nostr.Band API error: ${response.status}`);
		}

		const data: NostrBandResponse = await response.json();

		// Check if data.notes exists
		if (!data.notes || !Array.isArray(data.notes)) {
			console.warn('Invalid response format from nostr.band:', data);
			return [];
		}

		// Parse and transform the notes
		const parsedNotes: TrendingNote[] = data.notes.map((note) => {
			// Parse the author's content field to get profile data
			let profile;
			try {
				profile = JSON.parse(note.author.content);
			} catch (e) {
				console.warn('Failed to parse author profile:', e);
				profile = {};
			}

			return {
				event: note.event,
				author: note.author,
				profile: profile,
				stats: {
					replies: note.stats?.replies_count,
					reactions: note.stats?.reactions_count,
					reposts: note.stats?.reposts_count,
					zaps: note.stats?.zaps_msats ? Math.floor(note.stats.zaps_msats / 1000) : undefined
				}
			};
		});

		// Return limited or all trending notes
		return limit ? parsedNotes.slice(0, limit) : parsedNotes;
	} catch (error) {
		// Don't log AbortError as it's expected when queries are cancelled
		if (error instanceof Error && error.name === 'AbortError') {
			return [];
		}
		console.error('Failed to fetch trending notes:', error);
		return [];
	}
}
