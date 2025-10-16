import { Link, useParams } from 'react-router-dom';
import { nip19 } from 'nostr-tools';
import { useSeoMeta } from '@unhead/react';
import { ArrowLeft, Loader2 } from 'lucide-react';
import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery } from '@tanstack/react-query';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { PostCard } from '@/components/PostCard';
import { PostComposer } from '@/components/PostComposer';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { useAuthor } from '@/hooks/useAuthor';
import { genUserName } from '@/lib/genUserName';

export function ThreadPage() {
  const { nip19: noteIdParam } = useParams<{ nip19?: string }>();
  const { nostr } = useNostr();

  // Decode note1 to get event ID
  let eventId: string | undefined;
  let authorHint: string | undefined;
  let relaysHint: string[] | undefined;

  try {
    if (noteIdParam?.startsWith('note1')) {
      eventId = nip19.decode(noteIdParam).data as string;
    } else if (noteIdParam?.startsWith('nevent1')) {
      const decoded = nip19.decode(noteIdParam);
      const data = decoded.data as any;
      eventId = data.id;
      authorHint = data.author;
      relaysHint = data.relays;
    }
  } catch (error) {
    console.error('Failed to decode note ID:', error);
  }

  // Fetch the main event
  const { data: mainEvent, isLoading: mainLoading } = useQuery<NostrEvent | null>({
    queryKey: ['event', eventId],
    queryFn: async ({ signal }) => {
      if (!eventId) return null;

      const filters: any = { ids: [eventId], limit: 1 };

      const [event] = await nostr.query(
        [filters],
        { signal: AbortSignal.any([signal, AbortSignal.timeout(3000)]) }
      );

      return event || null;
    },
    enabled: !!eventId,
  });

  // Find parent event IDs from the main event's tags
  const parentEventIds = mainEvent?.tags
    .filter(tag => tag[0] === 'e')
    .map(tag => tag[1]) || [];

  // Fetch parent events to show thread context
  const { data: parentEvents, isLoading: parentsLoading } = useQuery<NostrEvent[]>({
    queryKey: ['thread-parents', parentEventIds],
    queryFn: async ({ signal }) => {
      if (parentEventIds.length === 0) return [];

      const events = await nostr.query(
        [{ ids: parentEventIds, kinds: [1] }],
        { signal: AbortSignal.any([signal, AbortSignal.timeout(3000)]) }
      );

      // Sort by created_at ascending (oldest first) to show conversation order
      return events.sort((a, b) => a.created_at - b.created_at);
    },
    enabled: parentEventIds.length > 0,
  });

  // Fetch replies
  const { data: replies, isLoading: repliesLoading } = useQuery<NostrEvent[]>({
    queryKey: ['replies', eventId],
    queryFn: async ({ signal }) => {
      if (!eventId) return [];

      const replyEvents = await nostr.query(
        [{ kinds: [1], '#e': [eventId], limit: 100 }],
        { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) }
      );

      // Sort by created_at ascending (oldest first)
      return replyEvents.sort((a, b) => a.created_at - b.created_at);
    },
    enabled: !!eventId,
  });

  const { data: author } = useAuthor(mainEvent?.pubkey);
  const displayName = author?.metadata?.name || genUserName(mainEvent?.pubkey || '');

  useSeoMeta({
    title: mainEvent
      ? `${displayName} on nostr.blue: "${mainEvent.content.slice(0, 50)}..."`
      : 'Post / nostr.blue',
    description: mainEvent?.content || 'View this post on nostr.blue',
  });

  if (!eventId) {
    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="min-h-screen flex items-center justify-center">
          <p className="text-muted-foreground">Invalid post identifier</p>
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
            <h1 className="text-xl font-bold">Post</h1>
          </div>
        </div>

        {/* Thread Posts */}
        {mainLoading || parentsLoading ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : mainEvent ? (
          <>
            {/* Show parent posts first (thread context) */}
            {parentEvents && parentEvents.length > 0 && (
              <div className="border-b-2 border-blue-500/20">
                {parentEvents.map((parentEvent) => (
                  <div key={parentEvent.id} className="relative">
                    <PostCard event={parentEvent} showThread={false} />
                    {/* Thread line indicator */}
                    <div className="absolute left-[40px] top-[60px] bottom-0 w-0.5 bg-border" />
                  </div>
                ))}
              </div>
            )}

            {/* Main post being viewed */}
            <PostCard event={mainEvent} showThread={false} />
            <Separator />

            {/* Reply Composer */}
            <div className="border-b border-border">
              <PostComposer
                placeholder="Post your reply"
                replyTo={mainEvent.id}
                autoFocus={false}
              />
            </div>

            {/* Replies */}
            {repliesLoading ? (
              <div className="flex items-center justify-center py-10">
                <Loader2 className="h-6 w-6 animate-spin text-blue-500" />
              </div>
            ) : replies && replies.length > 0 ? (
              <div>
                {replies.map((reply) => (
                  <PostCard key={reply.id} event={reply} />
                ))}
              </div>
            ) : (
              <div className="flex flex-col items-center justify-center py-10 px-4 text-center text-muted-foreground">
                <p>No replies yet</p>
                <p className="text-sm">Be the first to reply!</p>
              </div>
            )}
          </>
        ) : (
          <div className="flex items-center justify-center py-20">
            <p className="text-muted-foreground">Post not found</p>
          </div>
        )}
      </div>
    </MainLayout>
  );
}

export default ThreadPage;
