import { useEffect, useRef } from 'react';
import { useParams, Link } from 'react-router-dom';
import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { PostCard } from '@/components/PostCard';
import { useFollowSets } from '@/hooks/useFollowSets';
import { useFeed } from '@/hooks/useFeed';
import { Button } from '@/components/ui/button';
import { Loader2, RefreshCw, ArrowLeft, Users, Settings, Lock } from 'lucide-react';

export function UserListFeedPage() {
  const { listId } = useParams<{ listId: string }>();
  const { getFollowSet } = useFollowSets();
  const observerTarget = useRef<HTMLDivElement>(null);

  const followSet = getFollowSet(listId || '');

  useSeoMeta({
    title: followSet ? `${followSet.title} / nostr.blue` : 'List / nostr.blue',
    description: followSet?.description || 'View posts from this list',
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
    authors: followSet?.pubkeys || [],
    excludeReplies: true,
  });

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

  const allEvents = data?.pages.flatMap((page) => page) || [];

  if (!followSet) {
    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="min-h-screen">
          <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
            <div className="flex items-center gap-4 p-4">
              <Link to="/lists">
                <Button variant="ghost" size="icon" className="rounded-full">
                  <ArrowLeft className="h-5 w-5" />
                </Button>
              </Link>
              <h1 className="text-xl font-bold">List not found</h1>
            </div>
          </div>
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <Users className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">List not found</h2>
            <p className="text-muted-foreground max-w-sm mb-4">
              This list doesn't exist or has been deleted.
            </p>
            <Link to="/lists">
              <Button>Go to Lists</Button>
            </Link>
          </div>
        </div>
      </MainLayout>
    );
  }

  return (
    <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
      <div className="min-h-screen">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
          <div className="flex items-center justify-between px-4 py-3">
            <div className="flex items-center gap-3 flex-1 min-w-0">
              <Link to="/lists">
                <Button variant="ghost" size="icon" className="rounded-full flex-shrink-0">
                  <ArrowLeft className="h-5 w-5" />
                </Button>
              </Link>
              <div className="flex-1 min-w-0">
                <h1 className="text-xl font-bold truncate flex items-center gap-2">
                  {followSet.title}
                  {followSet.isPrivate && <Lock className="h-4 w-4 text-muted-foreground flex-shrink-0" />}
                </h1>
                <p className="text-sm text-muted-foreground">
                  {followSet.pubkeys.length} {followSet.pubkeys.length === 1 ? 'person' : 'people'}
                  {followSet.isPrivate && ' • Private'}
                </p>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <Link to={`/lists/${listId}/manage`}>
                <Button variant="ghost" size="icon" className="flex-shrink-0">
                  <Settings className="h-5 w-5" />
                </Button>
              </Link>
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

          {followSet.description && (
            <div className="px-4 pb-3">
              <p className="text-sm text-muted-foreground">{followSet.description}</p>
            </div>
          )}
        </div>

        {/* Feed */}
        {isLoading ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : followSet.pubkeys.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <Users className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">No people in this list</h2>
            <p className="text-muted-foreground max-w-sm mb-4">
              Add people to this list to see their posts here.
            </p>
            <Link to={`/lists/${listId}/manage`}>
              <Button>Manage List</Button>
            </Link>
          </div>
        ) : allEvents.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <div className="text-6xl mb-4">📭</div>
            <h2 className="text-2xl font-bold mb-2">No posts yet</h2>
            <p className="text-muted-foreground max-w-sm">
              There are no posts from people in this list yet. Check back later!
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

export default UserListFeedPage;
