import { useInfiniteQuery } from '@tanstack/react-query';
import type { NostrEvent } from '@nostrify/nostrify';

interface NostrWineSearchResult {
  data: NostrEvent[];
  pagination: {
    last_page: boolean;
    limit: number;
    next_url: string | null;
    page: number;
    total_pages: number;
    total_records: number;
  };
}

interface UseNostrWineSearchOptions {
  query: string;
  kinds?: number[];
  limit?: number;
  sort?: 'time' | 'relevance' | 'first_seen';
  order?: 'ascending' | 'descending';
}

/**
 * Hook to search nostr events using the nostr.wine search API
 * https://api.nostr.wine/search
 */
export function useNostrWineSearch(options: UseNostrWineSearchOptions) {
  const { query, kinds = [1], limit = 20, sort = 'relevance', order = 'descending' } = options;

  return useInfiniteQuery<NostrEvent[]>({
    queryKey: ['nostr-wine-search', { query, kinds, sort, order }],
    queryFn: async ({ pageParam = 1, signal }) => {
      if (!query.trim()) {
        return [];
      }

      const params = new URLSearchParams({
        query: query.trim(),
        kind: kinds.join(','),
        limit: limit.toString(),
        page: (pageParam as number).toString(),
        sort,
      });

      // Only add order param if sorting by time or first_seen
      if (sort === 'time' || sort === 'first_seen') {
        params.append('order', order);
      }

      const response = await fetch(
        `https://api.nostr.wine/search?${params.toString()}`,
        { signal }
      );

      if (!response.ok) {
        throw new Error('Failed to search events');
      }

      const data: NostrWineSearchResult = await response.json();

      // Store pagination metadata on the result array
      const events = data.data || [];
      (events as NostrEvent[] & { __pagination?: NostrWineSearchResult['pagination'] }).__pagination = data.pagination;

      return events;
    },
    getNextPageParam: (lastPage) => {
      // Check if there's a next page using the pagination metadata
      const pagination = (lastPage as NostrEvent[] & { __pagination?: NostrWineSearchResult['pagination'] }).__pagination;

      if (pagination && !pagination.last_page) {
        return pagination.page + 1;
      }

      return undefined;
    },
    initialPageParam: 1,
    enabled: !!query.trim(), // Only run query if search query is not empty
    staleTime: 5 * 60 * 1000, // Cache for 5 minutes
    gcTime: 10 * 60 * 1000, // Keep in cache for 10 minutes
    retry: 1,
  });
}
