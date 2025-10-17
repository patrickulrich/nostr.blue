import { useEffect, useRef } from 'react';
import { useParams } from 'react-router-dom';
import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { PostCard } from '@/components/PostCard';
import { useFeed } from '@/hooks/useFeed';
import { Button } from '@/components/ui/button';
import { Loader2, RefreshCw, Hash } from 'lucide-react';

/**
 * Page component that displays a feed of posts filtered by a specific hashtag.
 * Uses the useFeed hook with hashtag filtering to query events with the 't' tag.
 */
export function HashtagFeedPage() {
  const { tag } = useParams<{ tag: string }>();
  const hashtag = tag || '';

  useSeoMeta({
    title: hashtag ? `#${hashtag} / nostr.blue` : 'Hashtag / nostr.blue',
    description: hashtag ? `Posts tagged with #${hashtag} on Nostr` : 'Browse posts by hashtag',
  });

  const {
    data,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    isLoading,
    refetch,
    isRefetching,
  } = useFeed({
    hashtag,
    kinds: [1], // Short text notes
  }, {
    enabled: !!hashtag, // Only query if hashtag is valid
  });

  const observerTarget = useRef<HTMLDivElement>(null);

  // Infinite scroll
  useEffect(() => {
    if (!hashtag) return;

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
  }, [fetchNextPage, hasNextPage, isFetchingNextPage, hashtag]);

  const allEvents = data?.pages.flatMap((page) => page) || [];

  // Validate hashtag parameter - render error if invalid
  if (!hashtag) {
    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="flex flex-col items-center justify-center py-20 px-4 text-center min-h-screen">
          <Hash className="h-16 w-16 text-muted-foreground mb-4" />
          <h2 className="text-2xl font-bold mb-2">Invalid hashtag</h2>
          <p className="text-muted-foreground max-w-sm">
            Please provide a valid hashtag to search for.
          </p>
        </div>
      </MainLayout>
    );
  }

  return (
    <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
      <div className="min-h-screen">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
          <div className="flex items-center justify-between p-4">
            <div className="flex items-center gap-3">
              <Hash className="h-6 w-6 text-blue-500" />
              <div>
                <h1 className="text-xl font-bold">#{hashtag}</h1>
                <p className="text-sm text-muted-foreground">
                  Posts tagged with this hashtag
                </p>
              </div>
            </div>
            <Button
              variant="ghost"
              size="icon"
              onClick={() => refetch()}
              disabled={isRefetching}
            >
              <RefreshCw className={`h-5 w-5 ${isRefetching ? 'animate-spin' : ''}`} />
            </Button>
          </div>
        </div>

        {/* Feed */}
        {isLoading ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : allEvents.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <Hash className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">No posts found</h2>
            <p className="text-muted-foreground max-w-sm">
              There are no posts with #{hashtag} yet. Be the first to post about this topic!
            </p>
          </div>
        ) : (
          <>
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

export default HashtagFeedPage;
