import { useState } from 'react';
import { useParams, useNavigate, Link } from 'react-router-dom';
import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { useFollowSets } from '@/hooks/useFollowSets';
import { useAuthor } from '@/hooks/useAuthor';
import { useSearchUsers } from '@/hooks/useSearchUsers';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { ArrowLeft, Loader2, Trash2, UserPlus, X, Search, Lock, Unlock, Eye, EyeOff } from 'lucide-react';
import { useToast } from '@/hooks/useToast';
import { nip19 } from 'nostr-tools';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';

export function ManageListPage() {
  const { listId } = useParams<{ listId: string }>();
  const navigate = useNavigate();
  const { toast } = useToast();

  const {
    getFollowSet,
    updateFollowSet,
    addToFollowSet,
    removeFromFollowSet,
    deleteFollowSet,
    toggleListPrivacy,
    toggleMemberPrivacy,
  } = useFollowSets();

  const followSet = getFollowSet(listId || '');

  const [title, setTitle] = useState(followSet?.title || '');
  const [description, setDescription] = useState(followSet?.description || '');
  const [isUpdating, setIsUpdating] = useState(false);
  const [isTogglingPrivacy, setIsTogglingPrivacy] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  const [searchQuery, setSearchQuery] = useState('');
  const [isAddingMember, setIsAddingMember] = useState(false);
  const { data: searchResults = [], isLoading: isSearching } = useSearchUsers(searchQuery);

  useSeoMeta({
    title: followSet ? `Manage ${followSet.title} / nostr.blue` : 'Manage List / nostr.blue',
    description: 'Manage your custom list',
  });

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
            <p className="text-muted-foreground">This list doesn't exist or has been deleted.</p>
            <Link to="/lists">
              <Button className="mt-4">Go to Lists</Button>
            </Link>
          </div>
        </div>
      </MainLayout>
    );
  }

  const handleUpdateMetadata = async () => {
    if (!title.trim()) {
      toast({
        title: 'Error',
        description: 'List name cannot be empty',
        variant: 'destructive',
      });
      return;
    }

    setIsUpdating(true);
    try {
      await updateFollowSet.mutateAsync({
        id: listId || '',
        title: title.trim(),
        description: description.trim() || undefined,
      });

      toast({
        title: 'List updated',
        description: 'Your list has been updated successfully.',
      });
    } catch (error) {
      console.error('Failed to update list:', error);
      toast({
        title: 'Error',
        description: 'Failed to update list. Please try again.',
        variant: 'destructive',
      });
    } finally {
      setIsUpdating(false);
    }
  };

  const handleToggleListPrivacy = async () => {
    if (!followSet) return;

    setIsTogglingPrivacy(true);
    try {
      await toggleListPrivacy.mutateAsync({
        listId: listId || '',
        isPrivate: !followSet.isPrivate,
      });

      toast({
        title: followSet.isPrivate ? 'List is now public' : 'List is now private',
        description: followSet.isPrivate
          ? 'All members are now publicly visible'
          : 'All members are now encrypted and private',
      });
    } catch (error) {
      console.error('Failed to toggle list privacy:', error);
      toast({
        title: 'Error',
        description: 'Failed to update privacy setting. Please try again.',
        variant: 'destructive',
      });
    } finally {
      setIsTogglingPrivacy(false);
    }
  };

  const handleToggleMemberPrivacy = async (pubkey: string) => {
    try {
      await toggleMemberPrivacy.mutateAsync({
        listId: listId || '',
        pubkey,
      });

      const isCurrentlyPrivate = followSet?.privatePubkeys.includes(pubkey);
      toast({
        title: isCurrentlyPrivate ? 'Member is now public' : 'Member is now private',
        description: isCurrentlyPrivate
          ? 'This member is now publicly visible'
          : 'This member is now encrypted',
      });
    } catch (error) {
      console.error('Failed to toggle member privacy:', error);
      toast({
        title: 'Error',
        description: 'Failed to update member privacy. Please try again.',
        variant: 'destructive',
      });
    }
  };

  const handleAddMember = async (pubkey: string) => {
    setIsAddingMember(true);
    try {
      await addToFollowSet.mutateAsync({
        listId: listId || '',
        pubkey,
      });

      toast({
        title: 'Member added',
        description: 'User has been added to the list.',
      });

      setSearchQuery('');
    } catch (error) {
      console.error('Failed to add member:', error);
      toast({
        title: 'Error',
        description: 'Failed to add member. Please try again.',
        variant: 'destructive',
      });
    } finally {
      setIsAddingMember(false);
    }
  };

  const handleRemoveMember = async (pubkey: string) => {
    try {
      await removeFromFollowSet.mutateAsync({
        listId: listId || '',
        pubkey,
      });

      toast({
        title: 'Member removed',
        description: 'User has been removed from the list.',
      });
    } catch (error) {
      console.error('Failed to remove member:', error);
      toast({
        title: 'Error',
        description: 'Failed to remove member. Please try again.',
        variant: 'destructive',
      });
    }
  };

  const handleDeleteList = async () => {
    setIsDeleting(true);
    try {
      await deleteFollowSet.mutateAsync(listId || '');

      toast({
        title: 'List deleted',
        description: 'Your list has been deleted successfully.',
      });

      navigate('/lists');
    } catch (error) {
      console.error('Failed to delete list:', error);
      toast({
        title: 'Error',
        description: 'Failed to delete list. Please try again.',
        variant: 'destructive',
      });
      setIsDeleting(false);
    }
  };

  const handleAddMemberByNpub = async () => {
    try {
      const decoded = nip19.decode(searchQuery.trim());
      if (decoded.type === 'npub') {
        await handleAddMember(decoded.data);
      } else if (decoded.type === 'nprofile') {
        await handleAddMember(decoded.data.pubkey);
      } else {
        toast({
          title: 'Invalid input',
          description: 'Please enter a valid npub or nprofile',
          variant: 'destructive',
        });
      }
    } catch {
      // Not a valid nip19, ignore
      toast({
        title: 'Invalid input',
        description: 'Please enter a valid npub or search for a user',
        variant: 'destructive',
      });
    }
  };

  return (
    <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
      <div className="min-h-screen">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
          <div className="flex items-center gap-4 p-4">
            <Link to={`/lists/${listId}`}>
              <Button variant="ghost" size="icon" className="rounded-full">
                <ArrowLeft className="h-5 w-5" />
              </Button>
            </Link>
            <div>
              <h1 className="text-xl font-bold">Manage List</h1>
              <p className="text-sm text-muted-foreground">{followSet.title}</p>
            </div>
          </div>
        </div>

        <div className="max-w-2xl mx-auto p-4 space-y-6">
          {/* Edit Metadata */}
          <Card>
            <CardHeader>
              <CardTitle>List Details</CardTitle>
              <CardDescription>Update the name and description of your list</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="list-title">List name *</Label>
                <Input
                  id="list-title"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  maxLength={50}
                  placeholder="e.g., Developers, Friends, News"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="list-description">Description (optional)</Label>
                <Textarea
                  id="list-description"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  maxLength={200}
                  rows={3}
                  placeholder="What is this list for?"
                />
              </div>
              <Button
                onClick={handleUpdateMetadata}
                disabled={isUpdating || !title.trim()}
              >
                {isUpdating ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Updating...
                  </>
                ) : (
                  'Save Changes'
                )}
              </Button>
            </CardContent>
          </Card>

          {/* Privacy Settings */}
          <Card>
            <CardHeader>
              <CardTitle>Privacy Settings</CardTitle>
              <CardDescription>
                Control who can see the members of this list
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="flex items-center justify-between rounded-lg border p-4">
                <div className="flex items-center gap-3">
                  {followSet.isPrivate ? (
                    <Lock className="h-5 w-5 text-muted-foreground" />
                  ) : (
                    <Unlock className="h-5 w-5 text-muted-foreground" />
                  )}
                  <div className="space-y-0.5">
                    <Label htmlFor="list-privacy-toggle" className="cursor-pointer font-semibold">
                      {followSet.isPrivate ? 'Private List' : 'Public List'}
                    </Label>
                    <p className="text-sm text-muted-foreground">
                      {followSet.isPrivate
                        ? `${followSet.privatePubkeys.length} encrypted members, only visible to you`
                        : `${followSet.publicPubkeys.length} public members, visible to everyone`}
                    </p>
                    {followSet.isPrivate && followSet.publicPubkeys.length > 0 && (
                      <p className="text-sm text-orange-600">
                        Mixed mode: {followSet.publicPubkeys.length} public, {followSet.privatePubkeys.length} private
                      </p>
                    )}
                  </div>
                </div>
                <Switch
                  id="list-privacy-toggle"
                  checked={followSet.isPrivate}
                  onCheckedChange={handleToggleListPrivacy}
                  disabled={isTogglingPrivacy}
                />
              </div>
              {(followSet.publicPubkeys.length > 0 && followSet.privatePubkeys.length > 0) && (
                <div className="mt-4 rounded-lg bg-muted p-3">
                  <p className="text-sm text-muted-foreground">
                    <strong>Mixed Mode:</strong> This list has both public and private members.
                    You can toggle individual member privacy below, or use the switch above to convert all members at once.
                  </p>
                </div>
              )}
            </CardContent>
          </Card>

          {/* Add Members */}
          <Card>
            <CardHeader>
              <CardTitle>Add Members</CardTitle>
              <CardDescription>
                Search for users by name or paste an npub to add them to the list
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex gap-2">
                <div className="relative flex-1">
                  <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                  <Input
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    placeholder="Search users or paste npub..."
                    className="pl-9"
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && searchQuery.startsWith('npub')) {
                        handleAddMemberByNpub();
                      }
                    }}
                  />
                </div>
                {searchQuery.startsWith('npub') && (
                  <Button
                    onClick={handleAddMemberByNpub}
                    disabled={isAddingMember}
                  >
                    <UserPlus className="h-4 w-4" />
                  </Button>
                )}
              </div>

              {/* Search Results */}
              {searchQuery && !searchQuery.startsWith('npub') && (
                <div className="border rounded-lg overflow-hidden">
                  {isSearching ? (
                    <div className="p-4 text-center">
                      <Loader2 className="h-5 w-5 animate-spin mx-auto text-blue-500" />
                    </div>
                  ) : searchResults.length === 0 ? (
                    <div className="p-4 text-center text-sm text-muted-foreground">
                      No users found
                    </div>
                  ) : (
                    <div className="divide-y">
                      {searchResults.map((result) => (
                        <UserSearchResult
                          key={result.pubkey}
                          pubkey={result.pubkey}
                          isInList={followSet.pubkeys.includes(result.pubkey)}
                          onAdd={() => handleAddMember(result.pubkey)}
                          isAdding={isAddingMember}
                        />
                      ))}
                    </div>
                  )}
                </div>
              )}
            </CardContent>
          </Card>

          {/* List Members */}
          <Card>
            <CardHeader>
              <CardTitle>Members ({followSet.pubkeys.length})</CardTitle>
              <CardDescription>People in this list</CardDescription>
            </CardHeader>
            <CardContent>
              {followSet.pubkeys.length === 0 ? (
                <div className="text-center py-8 text-muted-foreground">
                  No members yet. Add some people to get started!
                </div>
              ) : (
                <div className="divide-y">
                  {followSet.members.map((member) => (
                    <ListMember
                      key={member.pubkey}
                      pubkey={member.pubkey}
                      isPrivate={member.isPrivate || false}
                      onRemove={() => handleRemoveMember(member.pubkey)}
                      onTogglePrivacy={() => handleToggleMemberPrivacy(member.pubkey)}
                    />
                  ))}
                </div>
              )}
            </CardContent>
          </Card>

          {/* Delete List */}
          <Card className="border-destructive">
            <CardHeader>
              <CardTitle className="text-destructive">Danger Zone</CardTitle>
              <CardDescription>Permanently delete this list</CardDescription>
            </CardHeader>
            <CardContent>
              <Button
                variant="destructive"
                onClick={() => setDeleteDialogOpen(true)}
              >
                <Trash2 className="mr-2 h-4 w-4" />
                Delete List
              </Button>
            </CardContent>
          </Card>
        </div>

        {/* Delete Confirmation Dialog */}
        <AlertDialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Are you sure?</AlertDialogTitle>
              <AlertDialogDescription>
                This will permanently delete the list "{followSet.title}". This action cannot be undone.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={isDeleting}>Cancel</AlertDialogCancel>
              <AlertDialogAction
                onClick={handleDeleteList}
                disabled={isDeleting}
                className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              >
                {isDeleting ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Deleting...
                  </>
                ) : (
                  'Delete List'
                )}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </MainLayout>
  );
}

function ListMember({
  pubkey,
  isPrivate,
  onRemove,
  onTogglePrivacy,
}: {
  pubkey: string;
  isPrivate: boolean;
  onRemove: () => void;
  onTogglePrivacy: () => void;
}) {
  const { data } = useAuthor(pubkey);
  const metadata = data?.metadata;

  return (
    <div className="flex items-center justify-between py-3 gap-2">
      <Link to={`/${nip19.npubEncode(pubkey)}`} className="flex items-center gap-3 flex-1 min-w-0">
        <img
          src={metadata?.picture || `https://api.dicebear.com/7.x/identicon/svg?seed=${pubkey}`}
          alt={metadata?.name || 'User'}
          className="w-10 h-10 rounded-full"
        />
        <div className="flex-1 min-w-0">
          <div className="font-semibold truncate flex items-center gap-2">
            {metadata?.display_name || metadata?.name || 'Anonymous'}
            {isPrivate && (
              <Lock className="h-3 w-3 text-muted-foreground" />
            )}
          </div>
          <div className="text-sm text-muted-foreground truncate">
            @{metadata?.name || nip19.npubEncode(pubkey).slice(0, 12)}
          </div>
        </div>
      </Link>
      <div className="flex items-center gap-1">
        <Button
          variant="ghost"
          size="icon"
          onClick={onTogglePrivacy}
          className="flex-shrink-0"
          title={isPrivate ? 'Make public' : 'Make private'}
        >
          {isPrivate ? <Eye className="h-4 w-4" /> : <EyeOff className="h-4 w-4" />}
        </Button>
        <Button
          variant="ghost"
          size="icon"
          onClick={onRemove}
          className="flex-shrink-0"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}

function UserSearchResult({
  pubkey,
  isInList,
  onAdd,
  isAdding,
}: {
  pubkey: string;
  isInList: boolean;
  onAdd: () => void;
  isAdding: boolean;
}) {
  const { data } = useAuthor(pubkey);
  const metadata = data?.metadata;

  return (
    <div className="flex items-center justify-between p-3 hover:bg-accent">
      <Link to={`/${nip19.npubEncode(pubkey)}`} className="flex items-center gap-3 flex-1 min-w-0">
        <img
          src={metadata?.picture || `https://api.dicebear.com/7.x/identicon/svg?seed=${pubkey}`}
          alt={metadata?.name || 'User'}
          className="w-10 h-10 rounded-full"
        />
        <div className="flex-1 min-w-0">
          <div className="font-semibold truncate">
            {metadata?.display_name || metadata?.name || 'Anonymous'}
          </div>
          <div className="text-sm text-muted-foreground truncate">
            @{metadata?.name || nip19.npubEncode(pubkey).slice(0, 12)}
          </div>
        </div>
      </Link>
      <Button
        size="sm"
        onClick={onAdd}
        disabled={isInList || isAdding}
        className="flex-shrink-0"
      >
        {isInList ? 'Added' : 'Add'}
      </Button>
    </div>
  );
}

export default ManageListPage;
