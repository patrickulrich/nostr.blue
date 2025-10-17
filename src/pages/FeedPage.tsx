import { useEffect, useRef, useState } from 'react';
import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { PostCard } from '@/components/PostCard';
import { PostComposer } from '@/components/PostComposer';
import { useFeed } from '@/hooks/useFeed';
import { useFollowing } from '@/hooks/useFollowing';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { Button } from '@/components/ui/button';
import { Loader2, RefreshCw } from 'lucide-react';
import { Separator } from '@/components/ui/separator';
import { cn } from '@/lib/utils';
import { useDVMJob } from '@/hooks/useDVMJob';
import { useNostr } from '@nostrify/react';
import { useQuery } from '@tanstack/react-query';
import { type NostrEvent } from '@nostrify/nostrify';

type FeedType = 'following' | 'popular';

// Popular DVM pubkey
const POPULAR_DVM_PUBKEY = '96945c769ef9e91be05570fef1003633f5bb9d072ba2453781b5140013ab35b3';

export function FeedPage() {
  const { user } = useCurrentUser();
  const { following } = useFollowing();
  const { nostr } = useNostr();
  const { useDVMFeed } = useDVMJob();
  const [feedType, setFeedType] = useState<FeedType>(user ? 'following' : 'popular');
  const [eventIds, setEventIds] = useState<string[]>([]);
  const [parsedDirectEvents, setParsedDirectEvents] = useState<NostrEvent[]>([]);

  // Update feed type when user logs in/out
  useEffect(() => {
    setFeedType(user ? 'following' : 'popular');
  }, [user]);

  useSeoMeta({
    title: 'Home / nostr.blue',
    description: 'A decentralized social network powered by Nostr',
  });

  // Fetch following feed
  const {
    data,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    isLoading: isLoadingFollowing,
    refetch: refetchFollowing,
    isRefetching: isRefetchingFollowing,
  } = useFeed({
    authors: feedType === 'following' && following.length > 0 ? following : undefined,
    excludeReplies: true,
  });

  // Fetch popular feed from DVM
  const { data: dvmFeedEvents, isLoading: isLoadingDVM, refetch: refetchDVM } = useDVMFeed(
    POPULAR_DVM_PUBKEY,
    5300,
    6300
  );

  // Parse DVM feed events - use only the most recent result for freshest recommendations
  useEffect(() => {
    if (feedType !== 'popular') return;

    const ids: string[] = [];
    const directEvents: NostrEvent[] = [];

    // Use only the most recent DVM result (first in sorted array)
    // Each result is a separate recommendation list, ranked by the DVM
    const mostRecentResult = dvmFeedEvents?.[0];

    if (mostRecentResult) {
      try {
        const content = mostRecentResult.content.trim();

        if (content.startsWith('[') || content.startsWith('{')) {
          const parsed = JSON.parse(content);

          if (Array.isArray(parsed)) {
            // Event IDs are in the DVM's intended order (likely by popularity)
            parsed.forEach(item => {
              if (Array.isArray(item) && item.length >= 2 && item[0] === 'e') {
                const eventId = item[1];
                if (typeof eventId === 'string' && eventId.length === 64) {
                  ids.push(eventId);
                }
              } else if (typeof item === 'object' && item.kind !== undefined) {
                directEvents.push(item as NostrEvent);
              } else if (typeof item === 'string' && item.length === 64) {
                ids.push(item);
              }
            });
          } else if (typeof parsed === 'object' && parsed.kind !== undefined) {
            directEvents.push(parsed as NostrEvent);
          }
        } else {
          const lines = content.split('\n').map(l => l.trim()).filter(l => l.length === 64);
          ids.push(...lines);
        }
      } catch (e) {
        console.error('Failed to parse DVM result:', e);
      }
    }

    setEventIds(ids);
    setParsedDirectEvents(directEvents);
  }, [dvmFeedEvents, feedType]);

  // Fetch full events from DVM results
  const { data: fetchedDVMEvents = [] } = useQuery<NostrEvent[]>({
    queryKey: ['popular-feed-events', eventIds],
    queryFn: async ({ signal }) => {
      if (eventIds.length === 0) return [];

      try {
        const events = await nostr.query(
          [{ ids: eventIds }],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(10000)]) }
        );

        // Sort by the DVM's ranking order (preserve order from eventIds)
        const eventMap = new Map(events.map(e => [e.id, e]));
        return eventIds
          .map(id => eventMap.get(id))
          .filter((e): e is NostrEvent => e !== undefined);
      } catch (error) {
        console.error('Failed to fetch events by IDs:', error);
        return [];
      }
    },
    enabled: eventIds.length > 0 && feedType === 'popular',
    staleTime: 60000,
  });

  const observerTarget = useRef<HTMLDivElement>(null);

  // Infinite scroll (only for following feed)
  useEffect(() => {
    if (feedType !== 'following') return;

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
  }, [fetchNextPage, hasNextPage, isFetchingNextPage, feedType]);

  // Combine events based on feed type
  const allEvents = feedType === 'following'
    ? (data?.pages.flatMap((page) => page) || [])
    : [...parsedDirectEvents, ...fetchedDVMEvents];

  const isLoading = feedType === 'following' ? isLoadingFollowing : isLoadingDVM;
  const isRefetching = feedType === 'following' ? isRefetchingFollowing : false;
  const refetch = feedType === 'following' ? refetchFollowing : refetchDVM;

  return (
    <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
      <div className="min-h-screen">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
          <div className="flex items-center justify-between px-4 pt-3">
            <div className="flex items-center gap-1 flex-1">
              {/* Feed Selector Tabs */}
              {user && (
                <>
                  <button
                    onClick={() => setFeedType('following')}
                    className={cn(
                      "flex-1 px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative",
                      feedType === 'following' ? 'text-foreground' : 'text-muted-foreground'
                    )}
                  >
                    Following
                    {feedType === 'following' && (
                      <div className="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full" />
                    )}
                  </button>
                  <button
                    onClick={() => setFeedType('popular')}
                    className={cn(
                      "flex-1 px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative",
                      feedType === 'popular' ? 'text-foreground' : 'text-muted-foreground'
                    )}
                  >
                    Popular
                    {feedType === 'popular' && (
                      <div className="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full" />
                    )}
                  </button>
                </>
              )}
              {!user && (
                <h1 className="text-xl font-bold px-4 py-3">Home</h1>
              )}
            </div>
            <Button
              variant="ghost"
              size="icon"
              onClick={() => refetch()}
              disabled={isRefetching}
              className="flex-shrink-0"
            >
              <RefreshCw className={`h-5 w-5 ${isRefetching ? 'animate-spin' : ''}`} />
            </Button>
          </div>
        </div>

        {/* Post Composer */}
        <PostComposer onSuccess={() => refetch()} />
        <Separator />

        {/* Feed */}
        {isLoading ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : allEvents.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <div className="text-6xl mb-4">👋</div>
            <h2 className="text-2xl font-bold mb-2">
              {feedType === 'following' && following.length === 0
                ? 'Follow some people!'
                : 'Welcome to nostr.blue!'}
            </h2>
            <p className="text-muted-foreground max-w-sm">
              {feedType === 'following' && following.length === 0
                ? 'You\'re not following anyone yet. Switch to the Global feed to discover people to follow, or search for profiles to get started.'
                : 'Your decentralized social feed. Connect to relays and start following people to see their posts here.'}
            </p>
          </div>
        ) : (
          <>
            {allEvents.map((event) => (
              <PostCard key={event.id} event={event} />
            ))}

            {/* Infinite scroll trigger (only for following feed) */}
            {feedType === 'following' && (
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
            )}

            {/* End message for popular feed */}
            {feedType === 'popular' && allEvents.length > 0 && (
              <div className="py-8 flex justify-center">
                <p className="text-muted-foreground text-sm">
                  You've reached the end
                </p>
              </div>
            )}
          </>
        )}
      </div>
    </MainLayout>
  );
}

export default FeedPage;
