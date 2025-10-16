import { useParams, Link } from 'react-router-dom';
import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { PostCard } from '@/components/PostCard';
import { useCommunity, useCommunityPosts } from '@/hooks/useCommunities';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import { Loader2, Users, ArrowLeft, RefreshCw } from 'lucide-react';
import { Badge } from '@/components/ui/badge';

export function CommunityFeedPage() {
  const { aTag } = useParams<{ aTag: string }>();
  const decodedATag = aTag ? decodeURIComponent(aTag) : '';

  const { data: community, isLoading: loadingCommunity } = useCommunity(decodedATag);
  const { data: posts, isLoading: loadingPosts, refetch } = useCommunityPosts(decodedATag);

  useSeoMeta({
    title: `${community?.name || 'Community'} / nostr.blue`,
    description: community?.description || 'A Nostr community',
  });

  if (loadingCommunity) {
    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="flex items-center justify-center py-20">
          <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
        </div>
      </MainLayout>
    );
  }

  if (!community) {
    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="p-8 text-center">
          <h2 className="text-2xl font-bold mb-4">Community Not Found</h2>
          <p className="text-muted-foreground mb-6">
            The requested community could not be found.
          </p>
          <Link to="/communities">
            <Button>
              <ArrowLeft className="h-4 w-4 mr-2" />
              Back to Communities
            </Button>
          </Link>
        </div>
      </MainLayout>
    );
  }

  return (
    <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
      <div className="min-h-screen">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
          <div className="p-4">
            <Link
              to="/communities"
              className="inline-flex items-center text-sm text-muted-foreground hover:text-foreground mb-3"
            >
              <ArrowLeft className="h-4 w-4 mr-1" />
              Back to Communities
            </Link>

            <div className="flex items-start gap-3 mb-3">
              <Avatar className="w-12 h-12">
                <AvatarImage src={community.image} alt={community.name || 'Community'} />
                <AvatarFallback>
                  <Users className="h-6 w-6" />
                </AvatarFallback>
              </Avatar>
              <div className="flex-1">
                <div className="flex items-center justify-between">
                  <h1 className="text-xl font-bold">{community.name || 'Unnamed Community'}</h1>
                  <Button
                    onClick={() => refetch()}
                    variant="ghost"
                    size="icon"
                  >
                    <RefreshCw className="h-5 w-5" />
                  </Button>
                </div>
                {community.description && (
                  <p className="text-sm text-muted-foreground mt-1">{community.description}</p>
                )}
                <div className="flex items-center gap-2 mt-2">
                  <Badge variant="secondary" className="text-xs">
                    {community.dTag}
                  </Badge>
                  {community.moderators.length > 0 && (
                    <Badge variant="outline" className="text-xs">
                      {community.moderators.length} moderator{community.moderators.length !== 1 ? 's' : ''}
                    </Badge>
                  )}
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Posts */}
        {loadingPosts ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : posts && posts.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <Users className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">No posts yet</h2>
            <p className="text-muted-foreground max-w-sm">
              This community doesn't have any posts yet. Be the first to post!
            </p>
          </div>
        ) : (
          <div>
            {posts?.map((event) => (
              <PostCard key={event.id} event={event} />
            ))}

            {posts && posts.length > 0 && (
              <div className="py-8 flex justify-center">
                <p className="text-muted-foreground text-sm">
                  You've reached the end
                </p>
              </div>
            )}
          </div>
        )}
      </div>
    </MainLayout>
  );
}

export default CommunityFeedPage;
