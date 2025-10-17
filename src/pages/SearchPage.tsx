import { useEffect, useRef, useState } from 'react';
import { useSeoMeta } from '@unhead/react';
import { useSearchParams } from 'react-router-dom';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { PostCard } from '@/components/PostCard';
import { useNostrWineSearch } from '@/hooks/useNostrWineSearch';
import { Loader2, Search as SearchIcon } from 'lucide-react';
import { Input } from '@/components/ui/input';

export function SearchPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const query = searchParams.get('q') || '';
  const [searchInput, setSearchInput] = useState(query);

  useSeoMeta({
    title: query ? `Search: ${query} / nostr.blue` : 'Search / nostr.blue',
    description: 'Search for posts on nostr.blue',
  });

  const {
    data,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    isLoading,
  } = useNostrWineSearch({
    query,
    kinds: [1], // Search for kind 1 notes (text posts)
    limit: 20,
    sort: 'relevance',
  });

  const observerTarget = useRef<HTMLDivElement>(null);

  // Infinite scroll
  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting && hasNextPage && !isFetchingNextPage) {
          fetchNextPage();
        }
      },
      { threshold: 0.1 }
    );

    const currentTarget = observerTarget.current;
    if (currentTarget) {
      observer.observe(currentTarget);
    }

    return () => {
      if (currentTarget) {
        observer.unobserve(currentTarget);
      }
    };
  }, [fetchNextPage, hasNextPage, isFetchingNextPage]);

  const allEvents = data?.pages.flatMap((page) => page.events) || [];

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (searchInput.trim()) {
      setSearchParams({ q: searchInput.trim() });
    }
  };

  return (
    <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
      <div className="min-h-screen">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
          <div className="p-4">
            <h1 className="text-xl font-bold flex items-center gap-2 mb-4">
              <SearchIcon className="h-5 w-5 text-blue-500" />
              Search
            </h1>
            <form onSubmit={handleSearch} className="relative">
              <SearchIcon className="absolute left-3 top-1/2 transform -translate-y-1/2 h-5 w-5 text-muted-foreground" />
              <Input
                type="text"
                placeholder="Search posts..."
                value={searchInput}
                onChange={(e) => setSearchInput(e.target.value)}
                className="pl-11 bg-muted/50 border-border rounded-full h-11 focus-visible:ring-1 focus-visible:ring-blue-500"
              />
            </form>
          </div>
        </div>

        {/* Results */}
        {!query ? (
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <SearchIcon className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">Search Nostr</h2>
            <p className="text-muted-foreground max-w-sm">
              Enter a search query above to find posts across the network.
            </p>
            <p className="text-xs text-muted-foreground mt-4">
              Powered by nostr.wine
            </p>
          </div>
        ) : isLoading ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : allEvents.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <SearchIcon className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">No results found</h2>
            <p className="text-muted-foreground max-w-sm">
              Try a different search query or check your spelling.
            </p>
          </div>
        ) : (
          <>
            <div className="px-4 py-3 text-sm text-muted-foreground border-b border-border">
              Found {allEvents.length} result{allEvents.length !== 1 ? 's' : ''} for "{query}"
            </div>
            {allEvents.map((event) => (
              <PostCard key={event.id} event={event} />
            ))}

            {/* Infinite scroll trigger */}
            <div ref={observerTarget} className="py-8 flex justify-center">
              {isFetchingNextPage && (
                <Loader2 className="h-6 w-6 animate-spin text-blue-500" />
              )}
              {!hasNextPage && allEvents.length > 0 && (
                <p className="text-muted-foreground text-sm">
                  You've reached the end
                </p>
              )}
            </div>
          </>
        )}
      </div>
    </MainLayout>
  );
}

export default SearchPage;
