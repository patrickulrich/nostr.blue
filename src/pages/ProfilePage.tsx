import { useEffect, useRef } from 'react';
import { useParams, Link } from 'react-router-dom';
import { nip19 } from 'nostr-tools';
import { useSeoMeta } from '@unhead/react';
import { ArrowLeft, Calendar, Settings2, Loader2 } from 'lucide-react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { PostCard } from '@/components/PostCard';
import { FollowButton } from '@/components/FollowButton';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import { useAuthor } from '@/hooks/useAuthor';
import { useFeed } from '@/hooks/useFeed';
import { useFollowing } from '@/hooks/useFollowing';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { useProfileStats } from '@/hooks/useProfileStats';
import { genUserName } from '@/lib/genUserName';
import { format } from 'date-fns';

/**
 * Profile page component displaying user information and posts.
 * Shows avatar, banner, bio, follower counts, and infinite-scrolling feed of user posts.
 */
export function ProfilePage() {
  const { nip19: nip19Param } = useParams<{ nip19?: string }>();
  const { user: currentUser } = useCurrentUser();

  // Decode npub to get pubkey
  let pubkey: string | undefined;
  try {
    if (nip19Param?.startsWith('npub1')) {
      pubkey = nip19.decode(nip19Param).data as string;
    } else if (nip19Param?.startsWith('nprofile1')) {
      const decoded = nip19.decode(nip19Param);
      pubkey = (decoded.data as { pubkey: string }).pubkey;
    }
  } catch (error) {
    console.error('Failed to decode nip19:', error);
  }

  const { data: author, isLoading: authorLoading } = useAuthor(pubkey);
  const { followingCount } = useFollowing(pubkey);
  const { data: profileStats } = useProfileStats(pubkey);

  // Fetch user's posts
  const {
    data,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    isLoading: postsLoading,
  } = useFeed({ authors: pubkey ? [pubkey] : [], limit: 20 });

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

  const displayName = author?.metadata?.name || genUserName(pubkey || '');
  const username = author?.metadata?.name || `@${nip19Param?.slice(0, 12)}...`;
  const bio = author?.metadata?.about;
  const avatarUrl = author?.metadata?.picture;
  const bannerUrl = author?.metadata?.banner;
  const website = author?.metadata?.website;

  const allPosts = data?.pages.flatMap((page) => page) || [];
  const isOwnProfile = currentUser?.pubkey === pubkey;

  useSeoMeta({
    title: `${displayName} (@${username}) / nostr.blue`,
    description: bio || `${displayName}'s profile on nostr.blue`,
  });

  const joinedDate = author?.event ? format(new Date(author.event.created_at * 1000), 'MMMM yyyy') : null;

  if (!pubkey) {
    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="min-h-screen flex items-center justify-center">
          <p className="text-muted-foreground">Invalid profile identifier</p>
        </div>
      </MainLayout>
    );
  }

  return (
    <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
      <div className="min-h-screen">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
          <div className="flex items-center gap-4 p-4">
            <Link to="/">
              <Button variant="ghost" size="icon" className="rounded-full">
                <ArrowLeft className="h-5 w-5" />
              </Button>
            </Link>
            <div>
              <h1 className="text-xl font-bold">{displayName}</h1>
              <p className="text-sm text-muted-foreground">{allPosts.length} posts</p>
            </div>
          </div>
        </div>

        {/* Banner */}
        <div className="relative">
          {bannerUrl ? (
            <img
              src={bannerUrl}
              alt="Banner"
              className="w-full h-48 object-cover bg-muted"
            />
          ) : (
            <div className="w-full h-48 bg-gradient-to-br from-blue-400 to-blue-600" />
          )}
        </div>

        {/* Profile Info */}
        <div className="px-4 pb-4">
          {/* Avatar & Actions */}
          <div className="flex items-start justify-between mb-4">
            <Avatar className="w-32 h-32 -mt-16 border-4 border-background">
              <AvatarImage src={avatarUrl} alt={displayName} />
              <AvatarFallback className="text-4xl">{displayName[0]?.toUpperCase() || 'A'}</AvatarFallback>
            </Avatar>

            <div className="mt-3 flex gap-2">
              {isOwnProfile ? (
                <Link to="/settings">
                  <Button variant="outline" className="rounded-full px-6">
                    <Settings2 className="h-4 w-4 mr-2" />
                    Edit Profile
                  </Button>
                </Link>
              ) : (
                <FollowButton pubkey={pubkey} />
              )}
            </div>
          </div>

          {/* Name & Username */}
          <div className="mb-3">
            <h2 className="text-2xl font-bold">{displayName}</h2>
            <p className="text-muted-foreground">{username}</p>
          </div>

          {/* Bio */}
          {bio && (
            <p className="text-base mb-3 whitespace-pre-wrap">{bio}</p>
          )}

          {/* Metadata */}
          <div className="flex flex-wrap gap-4 text-sm text-muted-foreground mb-3">
            {website && (
              <a
                href={website}
                target="_blank"
                rel="noopener noreferrer"
                className="hover:underline text-blue-500"
              >
                {website.replace(/^https?:\/\//, '')}
              </a>
            )}
            {joinedDate && (
              <span className="flex items-center gap-1">
                <Calendar className="h-4 w-4" />
                Joined {joinedDate}
              </span>
            )}
          </div>

          {/* Following / Followers */}
          <div className="flex gap-4 text-sm">
            <span>
              <strong className="font-bold text-foreground">{followingCount}</strong>{' '}
              <span className="text-muted-foreground">Following</span>
            </span>
            <span>
              <strong className="font-bold text-foreground">
                {profileStats?.followers_pubkey_count?.toLocaleString() || '0'}
              </strong>{' '}
              <span className="text-muted-foreground">Followers</span>
            </span>
          </div>
        </div>

        {/* Posts */}
        <div className="border-t border-border">
          {authorLoading || postsLoading ? (
            <div className="flex items-center justify-center py-20">
              <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
            </div>
          ) : allPosts.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
              <p className="text-xl font-bold mb-2">No posts yet</p>
              <p className="text-muted-foreground">
                {isOwnProfile ? "You haven't posted anything yet." : "This user hasn't posted anything yet."}
              </p>
            </div>
          ) : (
            <>
              {allPosts.map((event) => (
                <PostCard key={event.id} event={event} />
              ))}

              {/* Infinite scroll trigger */}
              <div ref={observerTarget} className="py-8 flex justify-center">
                {isFetchingNextPage && (
                  <Loader2 className="h-6 w-6 animate-spin text-blue-500" />
                )}
                {!hasNextPage && allPosts.length > 0 && (
                  <p className="text-muted-foreground text-sm">No more posts</p>
                )}
              </div>
            </>
          )}
        </div>
      </div>
    </MainLayout>
  );
}

export default ProfilePage;
