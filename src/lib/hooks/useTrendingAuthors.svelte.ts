import { createQuery } from '@tanstack/svelte-query';

export interface TrendingAuthor {
	pubkey: string;
	profile?: {
		name?: string;
		display_name?: string;
		picture?: string;
		nip05?: string;
		about?: string;
		banner?: string;
		website?: string;
		lud16?: string;
	};
	stats?: {
		followers_pubkey_count?: number;
		pub_following_pubkey_count?: number;
	};
}

interface NostrBandApiAuthor {
	pubkey: string;
	profile?: string; // JSON string containing profile data
	stats?: {
		followers_pubkey_count?: number;
		pub_following_pubkey_count?: number;
	};
}

interface NostrBandAuthorsResponse {
	profiles: NostrBandApiAuthor[];
}

/**
 * Hook to fetch trending authors from nostr.band API
 * Returns the top trending authors
 */
export function useTrendingAuthors(limit?: number) {
	return createQuery<TrendingAuthor[]>(() => ({
		queryKey: ['trending-authors', limit],
		queryFn: async ({ signal }) => {
			try {
				const response = await fetch('https://api.nostr.band/v0/trending/profiles', {
					signal
				});

				if (!response.ok) {
					throw new Error('Failed to fetch trending authors');
				}

				const data: NostrBandAuthorsResponse = await response.json();

				// Check if data.profiles exists
				if (!data.profiles || !Array.isArray(data.profiles)) {
					console.warn('Invalid response format from nostr.band:', data);
					return [];
				}

				// Parse and transform the authors
				const parsedAuthors: TrendingAuthor[] = data.profiles.map((author) => {
					// Parse the profile field to get profile data
					let profile;
					try {
						profile = author.profile ? JSON.parse(author.profile) : {};
					} catch (e) {
						console.warn('Failed to parse author profile:', e);
						profile = {};
					}

					return {
						pubkey: author.pubkey,
						profile: profile,
						stats: author.stats
					};
				});

				// Return limited or all trending authors
				return limit ? parsedAuthors.slice(0, limit) : parsedAuthors;
			} catch (error) {
				// Don't log AbortError as it's expected when queries are cancelled
				if (error instanceof Error && error.name === 'AbortError') {
					return [];
				}
				console.error('Failed to fetch trending authors:', error);
				return [];
			}
		},
		staleTime: 15 * 60 * 1000, // Cache for 15 minutes
		gcTime: 30 * 60 * 1000, // Keep in cache for 30 minutes
		retry: 1
	}));
}
