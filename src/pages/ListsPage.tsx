import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { ListCard, ListItemDisplay } from '@/components/ListCard';
import { CreateListDialog } from '@/components/CreateListDialog';
import { useMuteList } from '@/hooks/useMuteList';
import { usePinnedNotes } from '@/hooks/usePinnedNotes';
import { useBookmarks } from '@/hooks/useBookmarks';
import { useFollowSets } from '@/hooks/useFollowSets';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { Button } from '@/components/ui/button';
import { List, ArrowLeft, Plus } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';

type ListType = 'mute' | 'pinned' | 'bookmarks' | null;

export function ListsPage() {
  const { user } = useCurrentUser();
  const navigate = useNavigate();
  const [selectedList, setSelectedList] = useState<ListType>(null);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);

  const muteList = useMuteList();
  const pinnedNotes = usePinnedNotes();
  const bookmarks = useBookmarks();
  const { followSets } = useFollowSets();

  useSeoMeta({
    title: 'Lists / nostr.blue',
    description: 'Manage your Nostr lists',
  });

  // Show login prompt if not logged in
  if (!user) {
    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="min-h-screen">
          {/* Header */}
          <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
            <div className="flex items-center justify-between p-4">
              <h1 className="text-xl font-bold">Lists</h1>
            </div>
          </div>

          {/* Login prompt */}
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <List className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">Sign in to manage your lists</h2>
            <p className="text-muted-foreground max-w-sm">
              Connect your Nostr account to create and manage lists like mutes, pinned notes, and more.
            </p>
          </div>
        </div>
      </MainLayout>
    );
  }

  // Show list detail view
  if (selectedList) {
    let listData: { name: string; items: any[]; type: string };

    if (selectedList === 'mute') {
      listData = {
        name: 'Mute List',
        items: muteList.items,
        type: 'mute',
      };
    } else if (selectedList === 'pinned') {
      listData = {
        name: 'Pinned Notes',
        items: pinnedNotes.items,
        type: 'pinned',
      };
    } else if (selectedList === 'bookmarks') {
      listData = {
        name: 'Bookmarks',
        items: bookmarks.bookmarks,
        type: 'bookmarks',
      };
    } else {
      listData = { name: '', items: [], type: '' };
    }

    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="min-h-screen">
          {/* Header */}
          <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
            <div className="flex items-center gap-4 p-4">
              <Button
                variant="ghost"
                size="icon"
                onClick={() => setSelectedList(null)}
                className="rounded-full"
              >
                <ArrowLeft className="h-5 w-5" />
              </Button>
              <div>
                <h1 className="text-xl font-bold">{listData.name}</h1>
                <p className="text-sm text-muted-foreground">
                  {listData.items.length} {listData.items.length === 1 ? 'item' : 'items'}
                </p>
              </div>
            </div>
          </div>

          {/* List items */}
          <Card className="border-0 rounded-none">
            {listData.items.length === 0 ? (
              <CardContent className="py-20 text-center">
                <p className="text-muted-foreground">No items in this list yet</p>
              </CardContent>
            ) : (
              <CardContent className="p-0">
                {listData.items.map((item, index) => (
                  <ListItemDisplay
                    key={`${item.type}-${item.value}-${index}`}
                    type={item.type}
                    value={item.value}
                    relay={item.relay}
                  />
                ))}
              </CardContent>
            )}
          </Card>
        </div>
      </MainLayout>
    );
  }

  // Show list overview
  return (
    <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
      <div className="min-h-screen">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
          <div className="flex items-center justify-between p-4">
            <h1 className="text-xl font-bold">Lists</h1>
            <Button onClick={() => setCreateDialogOpen(true)} size="sm" className="rounded-full">
              <Plus className="h-4 w-4 mr-2" />
              Create List
            </Button>
          </div>
        </div>

        {/* Description */}
        <div className="p-4 border-b border-border">
          <p className="text-muted-foreground">
            Manage your Nostr lists. Lists help you organize content, mute unwanted posts, and showcase your favorite notes.
          </p>
        </div>

        {/* Custom User Lists (Follow Sets) */}
        {followSets.length > 0 && (
          <div className="p-4 border-b border-border">
            <h2 className="text-lg font-semibold mb-4">Your Lists</h2>
            <div className="grid gap-4">
              {followSets.map((set) => (
                <ListCard
                  key={set.id}
                  kind={30000}
                  name={set.title}
                  description={set.description || `${set.pubkeys.length} ${set.pubkeys.length === 1 ? 'person' : 'people'} in this list`}
                  itemCount={set.pubkeys.length}
                  icon="users"
                  isPrivate={set.isPrivate}
                  onClick={() => navigate(`/lists/${set.id}`)}
                />
              ))}
            </div>
          </div>
        )}

        {/* Standard Lists */}
        <div className="p-4">
          <h2 className="text-lg font-semibold mb-4">Standard Lists</h2>
          <div className="grid gap-4">
            <ListCard
              kind={10000}
              name="Mute List"
              description="Pubkeys, hashtags, words, and threads you want to hide from your feed"
              itemCount={muteList.items.length}
              icon="mute"
              onClick={() => setSelectedList('mute')}
            />

            <ListCard
              kind={10001}
              name="Pinned Notes"
              description="Notes you want to showcase on your profile"
              itemCount={pinnedNotes.items.length}
              icon="pin"
              onClick={() => setSelectedList('pinned')}
            />

            <ListCard
              kind={10003}
              name="Bookmarks"
              description="Posts, articles, and links you want to save for later"
              itemCount={bookmarks.bookmarks.length}
              icon="bookmark"
              onClick={() => setSelectedList('bookmarks')}
            />
          </div>
        </div>

        {/* Info about Sets */}
        <div className="p-4 border-t border-border">
          <Card className="bg-muted/50">
            <CardHeader>
              <CardTitle className="text-base">What are Lists?</CardTitle>
              <CardDescription className="mt-2">
                Lists in Nostr (NIP-51) allow you to organize and manage different types of content:
                <ul className="list-disc list-inside mt-2 space-y-1">
                  <li><strong>Custom Lists:</strong> Create feeds from specific groups of people</li>
                  <li><strong>Mute List:</strong> Hide unwanted content from your feed</li>
                  <li><strong>Pinned Notes:</strong> Highlight your favorite posts on your profile</li>
                  <li><strong>Bookmarks:</strong> Save content to read or reference later</li>
                </ul>
              </CardDescription>
            </CardHeader>
          </Card>
        </div>
      </div>

      {/* Create List Dialog */}
      <CreateListDialog open={createDialogOpen} onOpenChange={setCreateDialogOpen} />
    </MainLayout>
  );
}

export default ListsPage;
