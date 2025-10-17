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

interface SearchPageData {
  events: NostrEvent[];
  pagination: NostrWineSearchResult['pagination'];
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

  return useInfiniteQuery<SearchPageData>({
    queryKey: ['nostr-wine-search', { query, kinds, sort, order }],
    queryFn: async ({ pageParam = 1, signal }) => {
      if (!query.trim()) {
        return {
          events: [],
          pagination: {
            last_page: true,
            limit: 0,
            next_url: null,
            page: 1,
            total_pages: 0,
            total_records: 0,
          },
        };
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

      // Return properly typed data with pagination
      return {
        events: data.data || [],
        pagination: data.pagination,
      };
    },
    getNextPageParam: (lastPage) => {
      // Check if there's a next page using the pagination metadata
      if (lastPage.pagination && !lastPage.pagination.last_page) {
        return lastPage.pagination.page + 1;
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
