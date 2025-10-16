import { useParams } from 'react-router-dom';
import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { useDVMs } from '@/hooks/useDVMs';
import { useDVMJob } from '@/hooks/useDVMJob';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Loader2, Zap, RefreshCw, ArrowLeft } from 'lucide-react';
import { Link } from 'react-router-dom';
import { useState, useEffect, useCallback } from 'react';
import { PostCard } from '@/components/PostCard';
import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery } from '@tanstack/react-query';

const kindNames: Record<number, string> = {
  5050: 'Search',
  5200: 'Content Discovery',
  5250: 'User Discovery',
  5300: 'Content Discovery',
};

export function DVMFeedPage() {
  const { dvmId } = useParams<{ dvmId: string }>();
  const { dvms, isLoading: loadingDVMs } = useDVMs();
  const { user } = useCurrentUser();
  const { nostr } = useNostr();
  const { submitJob, useDVMFeed } = useDVMJob();
  const [jobRequestId, setJobRequestId] = useState<string | null>(null);
  const [eventIds, setEventIds] = useState<string[]>([]);
  const [parsedDirectEvents, setParsedDirectEvents] = useState<NostrEvent[]>([]);

  // Find the DVM
  const dvm = dvms.find(d => d.id === dvmId || d.pubkey === dvmId);

  // Get the first supported kind for content discovery (5300 is most common)
  const requestKind = dvm?.supportedKinds.find(k => [5300, 5200, 5050, 5250].includes(k)) || 5300;
  const resultKind = requestKind + 1000;

  // Fetch the feed from this DVM
  const { data: feedEvents, isLoading: loadingFeed, refetch } = useDVMFeed(
    dvm?.pubkey || '',
    requestKind,
    resultKind
  );

  useSeoMeta({
    title: `${dvm?.name || 'DVM'} Feed / nostr.blue`,
    description: `Feed from ${dvm?.name || 'Data Vending Machine'}`,
  });

  const handleRequestFeed = useCallback(async () => {
    if (!dvm || !user) return;

    try {
      const result = await submitJob.mutateAsync({
        kind: requestKind,
        targetPubkey: dvm.pubkey,
        params: {
          limit: '50',
        },
      });
      setJobRequestId(result.id);
    } catch (error) {
      console.error('Failed to request feed:', error);
    }
  }, [dvm, user, submitJob, requestKind]);

  // Auto-submit job request on mount if user is logged in
  useEffect(() => {
    if (dvm && user && !jobRequestId) {
      handleRequestFeed();
    }
  }, [dvm, user, jobRequestId, handleRequestFeed]);

  // Parse feed events - use only the most recent result for freshest recommendations
  useEffect(() => {
    const ids: string[] = [];
    const directEvents: NostrEvent[] = [];

    // Use only the most recent DVM result (first in sorted array)
    // Each result is a separate recommendation list, ranked by the DVM
    const mostRecentResult = feedEvents?.[0];

    if (mostRecentResult) {
      try {
        const content = mostRecentResult.content.trim();

        // Try parsing as JSON first
        if (content.startsWith('[') || content.startsWith('{')) {
          const parsed = JSON.parse(content);

          if (Array.isArray(parsed)) {
            // Event IDs are in the DVM's intended order (likely by popularity)
            parsed.forEach(item => {
              // Check if it's a tag array (DVM format: [["e", "id"], ["e", "id"]])
              if (Array.isArray(item) && item.length >= 2 && item[0] === 'e') {
                // This is an "e" tag containing an event ID
                const eventId = item[1];
                if (typeof eventId === 'string' && eventId.length === 64) {
                  ids.push(eventId);
                }
              }
              // Full event object
              else if (typeof item === 'object' && item.kind !== undefined) {
                directEvents.push(item as NostrEvent);
              }
              // Event ID string
              else if (typeof item === 'string' && item.length === 64) {
                ids.push(item);
              }
            });
          } else if (typeof parsed === 'object' && parsed.kind !== undefined) {
            // Single event object
            directEvents.push(parsed as NostrEvent);
          }
        } else {
          // Plain text - might be newline-separated event IDs
          const lines = content.split('\n').map(l => l.trim()).filter(l => l.length === 64);
          ids.push(...lines);
        }
      } catch (e) {
        console.error('Failed to parse DVM result:', e, mostRecentResult.content);
      }
    }

    setEventIds(ids);
    setParsedDirectEvents(directEvents);
  }, [feedEvents]);

  // Fetch full events if we have event IDs
  const { data: fetchedEvents = [] } = useQuery<NostrEvent[]>({
    queryKey: ['dvm-feed-events', eventIds],
    queryFn: async ({ signal }) => {
      if (eventIds.length === 0) return [];

      console.log('[DVMFeedPage] Querying relay for', eventIds.length, 'event IDs');

      try {
        const events = await nostr.query(
          [{ ids: eventIds }],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(10000)]) }
        );

        console.log('[DVMFeedPage] Relay returned', events.length, 'events');

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
    enabled: eventIds.length > 0,
    staleTime: 60000,
  });

  // Combine directly parsed events and fetched events
  const parsedEvents = [...parsedDirectEvents, ...fetchedEvents];

  if (loadingDVMs) {
    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="flex items-center justify-center py-20">
          <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
        </div>
      </MainLayout>
    );
  }

  if (!dvm) {
    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="p-8 text-center">
          <h2 className="text-2xl font-bold mb-4">DVM Not Found</h2>
          <p className="text-muted-foreground mb-6">
            The requested Data Vending Machine could not be found.
          </p>
          <Link to="/dvm">
            <Button>
              <ArrowLeft className="h-4 w-4 mr-2" />
              Back to DVMs
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
            <Link to="/dvm" className="inline-flex items-center text-sm text-muted-foreground hover:text-foreground mb-3">
              <ArrowLeft className="h-4 w-4 mr-1" />
              Back to DVMs
            </Link>

            <div className="flex items-start gap-3 mb-3">
              <Avatar className="w-12 h-12">
                <AvatarImage src={dvm.picture} alt={dvm.name || 'DVM'} />
                <AvatarFallback>
                  <Zap className="h-6 w-6" />
                </AvatarFallback>
              </Avatar>
              <div className="flex-1">
                <h1 className="text-xl font-bold">{dvm.name || 'Unnamed DVM'}</h1>
                {dvm.about && (
                  <p className="text-sm text-muted-foreground mt-1">{dvm.about}</p>
                )}
                <div className="flex flex-wrap gap-2 mt-2">
                  {dvm.supportedKinds.filter(k => [5050, 5200, 5250, 5300].includes(k)).map(kind => (
                    <Badge key={kind} variant="secondary" className="text-xs">
                      {kindNames[kind] || `Kind ${kind}`}
                    </Badge>
                  ))}
                </div>
              </div>
            </div>

            <div className="flex gap-2">
              <Button
                onClick={handleRequestFeed}
                disabled={!user || submitJob.isPending}
                className="gap-2"
                size="sm"
              >
                {submitJob.isPending ? (
                  <>
                    <Loader2 className="h-4 w-4 animate-spin" />
                    Requesting...
                  </>
                ) : (
                  <>
                    <Zap className="h-4 w-4" />
                    Request Feed
                  </>
                )}
              </Button>
              <Button
                onClick={() => refetch()}
                disabled={loadingFeed}
                variant="outline"
                size="sm"
                className="gap-2"
              >
                <RefreshCw className={`h-4 w-4 ${loadingFeed ? 'animate-spin' : ''}`} />
                Refresh
              </Button>
            </div>
          </div>
        </div>

        {/* Feed Content */}
        {!user ? (
          <div className="p-8 text-center">
            <Card>
              <CardContent className="py-12">
                <Zap className="h-12 w-12 text-muted-foreground mx-auto mb-4" />
                <h3 className="text-lg font-semibold mb-2">Login Required</h3>
                <p className="text-muted-foreground">
                  You need to be logged in to request feeds from DVMs.
                </p>
              </CardContent>
            </Card>
          </div>
        ) : loadingFeed && parsedEvents.length === 0 ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : parsedEvents.length === 0 ? (
          <div className="p-8 text-center">
            <Card>
              <CardContent className="py-12">
                <Zap className="h-12 w-12 text-muted-foreground mx-auto mb-4" />
                <h3 className="text-lg font-semibold mb-2">No Results Yet</h3>
                <p className="text-muted-foreground mb-4">
                  {feedEvents && feedEvents.length > 0
                    ? `Found ${feedEvents.length} DVM result(s) but couldn't parse them. Check console for details.`
                    : 'No feed results have been published by this DVM yet. Try requesting a feed or check back later.'}
                </p>
                {feedEvents && feedEvents.length > 0 && (
                  <p className="text-xs text-muted-foreground mb-4">
                    Debug info: {eventIds.length} event IDs found, {parsedDirectEvents.length} direct events parsed.
                    Open browser console (F12) to see raw DVM responses.
                  </p>
                )}
                {!jobRequestId && (
                  <Button onClick={handleRequestFeed} disabled={submitJob.isPending}>
                    <Zap className="h-4 w-4 mr-2" />
                    Request Feed
                  </Button>
                )}
              </CardContent>
            </Card>
          </div>
        ) : (
          <div>
            {parsedEvents.map((event) => (
              <PostCard key={event.id} event={event} />
            ))}
          </div>
        )}
      </div>
    </MainLayout>
  );
}

export default DVMFeedPage;
