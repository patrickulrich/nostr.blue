import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { BookmarkItem } from '@/components/BookmarkItem';
import { useBookmarks } from '@/hooks/useBookmarks';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { Button } from '@/components/ui/button';
import { Loader2, Bookmark } from 'lucide-react';
import { useToast } from '@/hooks/useToast';

export function BookmarksPage() {
  const { user } = useCurrentUser();
  const { bookmarks, isLoading, removeBookmark } = useBookmarks();
  const { toast } = useToast();

  useSeoMeta({
    title: 'Bookmarks / nostr.blue',
    description: 'Your saved bookmarks on nostr.blue',
  });

  const handleRemoveBookmark = async (type: string, value: string) => {
    try {
      await removeBookmark.mutateAsync({ type, value });
      toast({
        title: 'Bookmark removed',
        description: 'The bookmark has been removed from your list.',
      });
    } catch (error) {
      console.error('Failed to remove bookmark:', error);
      toast({
        title: 'Error',
        description: 'Failed to remove bookmark. Please try again.',
        variant: 'destructive',
      });
    }
  };

  // Show login prompt if not logged in
  if (!user) {
    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="min-h-screen">
          {/* Header */}
          <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
            <div className="flex items-center justify-between p-4">
              <h1 className="text-xl font-bold">Bookmarks</h1>
            </div>
          </div>

          {/* Login prompt */}
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <Bookmark className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">Sign in to see your bookmarks</h2>
            <p className="text-muted-foreground max-w-sm">
              Connect your Nostr account to save and view bookmarks of posts, articles, and more.
            </p>
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
          <div className="flex items-center justify-between p-4">
            <div>
              <h1 className="text-xl font-bold">Bookmarks</h1>
              <p className="text-sm text-muted-foreground">
                {bookmarks.length} {bookmarks.length === 1 ? 'bookmark' : 'bookmarks'}
              </p>
            </div>
          </div>
        </div>

        {/* Bookmarks Feed */}
        {isLoading ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : bookmarks.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <Bookmark className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">No bookmarks yet</h2>
            <p className="text-muted-foreground max-w-sm">
              Save posts, articles, hashtags, and links you want to revisit later. Click the bookmark icon on any post to add it here.
            </p>
          </div>
        ) : (
          <div>
            {bookmarks.map((bookmark, index) => (
              <BookmarkItem
                key={`${bookmark.type}-${bookmark.value}-${index}`}
                bookmark={bookmark}
                onRemove={() => handleRemoveBookmark(bookmark.type, bookmark.value)}
              />
            ))}
          </div>
        )}
      </div>
    </MainLayout>
  );
}

export default BookmarksPage;
