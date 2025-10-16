import { useMemo, useState } from 'react';
import { type NostrEvent } from '@nostrify/nostrify';
import { Link } from 'react-router-dom';
import { nip19 } from 'nostr-tools';
import { useNostr } from '@nostrify/react';
import { useQuery } from '@tanstack/react-query';
import { useAuthor } from '@/hooks/useAuthor';
import { genUserName } from '@/lib/genUserName';
import { cn } from '@/lib/utils';

interface NoteContentProps {
  event: NostrEvent;
  className?: string;
  depth?: number; // Prevent infinite embedding loops
}

// Helper to detect if URL is an image
function isImageUrl(url: string): boolean {
  return /\.(jpg|jpeg|png|gif|webp|svg|bmp|ico)(\?.*)?$/i.test(url);
}

// Helper to detect if URL is a video
function isVideoUrl(url: string): boolean {
  return /\.(mp4|webm|ogg|mov)(\?.*)?$/i.test(url);
}

// Component to render an image with loading state
function MediaImage({ src, alt }: { src: string; alt: string }) {
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState(false);

  if (error) {
    return (
      <a
        href={src}
        target="_blank"
        rel="noopener noreferrer"
        className="text-blue-500 hover:underline break-all"
      >
        {src}
      </a>
    );
  }

  return (
    <div className="my-2 rounded-lg overflow-hidden border border-border max-w-full">
      {!loaded && (
        <div className="w-full h-64 bg-muted animate-pulse flex items-center justify-center">
          <span className="text-muted-foreground text-sm">Loading image...</span>
        </div>
      )}
      <img
        src={src}
        alt={alt}
        className={cn(
          "max-w-full h-auto max-h-[500px] object-contain cursor-pointer",
          !loaded && "hidden"
        )}
        onLoad={() => setLoaded(true)}
        onError={() => setError(true)}
        onClick={() => window.open(src, '_blank')}
      />
    </div>
  );
}

// Component to render a video
function MediaVideo({ src }: { src: string }) {
  const [error, setError] = useState(false);

  if (error) {
    return (
      <a
        href={src}
        target="_blank"
        rel="noopener noreferrer"
        className="text-blue-500 hover:underline break-all"
      >
        {src}
      </a>
    );
  }

  return (
    <div className="my-2 rounded-lg overflow-hidden border border-border max-w-full">
      <video
        src={src}
        controls
        className="max-w-full h-auto max-h-[500px]"
        onError={() => setError(true)}
      />
    </div>
  );
}

// Embedded note component
function EmbeddedNote({ event }: { event: NostrEvent }) {
  const { data: author } = useAuthor(event.pubkey);
  const npub = nip19.npubEncode(event.pubkey);
  const noteId = nip19.noteEncode(event.id);

  const displayName = author?.metadata?.name || genUserName(event.pubkey);
  const username = author?.metadata?.name ? `@${author.metadata.name}` : `@${npub.slice(0, 12)}...`;
  const avatarUrl = author?.metadata?.picture;

  return (
    <div className="block border border-border rounded-xl p-3 hover:bg-accent/50 transition-colors my-3 cursor-pointer">
      <Link
        to={`/${noteId}`}
        className="flex items-start gap-2 mb-2 hover:underline"
        onClick={(e) => e.stopPropagation()}
      >
        {avatarUrl ? (
          <img src={avatarUrl} alt={displayName} className="w-5 h-5 rounded-full" />
        ) : (
          <div className="w-5 h-5 rounded-full bg-muted flex items-center justify-center text-xs">
            {displayName[0]?.toUpperCase()}
          </div>
        )}
        <div className="flex flex-col min-w-0">
          <span className="font-semibold text-sm truncate">{displayName}</span>
          <span className="text-xs text-muted-foreground truncate">{username}</span>
        </div>
      </Link>
      <NoteContent event={event} className="text-sm" depth={1} />
    </div>
  );
}

/** Parses content of text note events so that URLs and hashtags are linkified. */
export function NoteContent({
  event,
  className,
  depth = 0,
}: NoteContentProps) {
  const { nostr } = useNostr();

  // Extract note references from content
  const noteReferences = useMemo(() => {
    const refs: string[] = [];
    const regex = /nostr:(note1|nevent1)([023456789acdefghjklmnpqrstuvwxyz]+)/g;
    let match;

    while ((match = regex.exec(event.content)) !== null) {
      refs.push(`${match[1]}${match[2]}`);
    }

    return refs;
  }, [event.content]);

  // Fetch referenced events (only if depth is 0 to prevent infinite loops)
  const { data: referencedEvents } = useQuery<Record<string, NostrEvent>>({
    queryKey: ['referenced-notes', noteReferences],
    queryFn: async ({ signal }) => {
      if (noteReferences.length === 0 || depth > 0) return {};

      const eventIds: string[] = [];

      for (const ref of noteReferences) {
        try {
          const decoded = nip19.decode(ref);
          if (decoded.type === 'note') {
            eventIds.push(decoded.data as string);
          } else if (decoded.type === 'nevent') {
            eventIds.push((decoded.data as any).id);
          }
        } catch (error) {
          console.error('Failed to decode note reference:', error);
        }
      }

      if (eventIds.length === 0) return {};

      const events = await nostr.query(
        [{ ids: eventIds, kinds: [1] }],
        { signal: AbortSignal.any([signal, AbortSignal.timeout(3000)]) }
      );

      // Create a map of eventId -> event
      const eventMap: Record<string, NostrEvent> = {};
      events.forEach(evt => {
        eventMap[evt.id] = evt;
      });

      return eventMap;
    },
    enabled: noteReferences.length > 0 && depth === 0,
    staleTime: 60000,
  });

  // Process the content to render mentions, links, media, etc.
  const content = useMemo(() => {
    const text = event.content;

    // Regex to find URLs, Nostr references, and hashtags
    const regex = /(https?:\/\/[^\s]+)|nostr:(npub1|note1|nprofile1|nevent1)([023456789acdefghjklmnpqrstuvwxyz]+)|(#\w+)/g;

    const parts: React.ReactNode[] = [];
    const mediaUrls: string[] = [];
    const noteRefs: Array<{ index: number; ref: string }> = [];
    let lastIndex = 0;
    let match: RegExpExecArray | null;
    let keyCounter = 0;

    while ((match = regex.exec(text)) !== null) {
      const [fullMatch, url, nostrPrefix, nostrData, hashtag] = match;
      const index = match.index;

      // Add text before this match
      if (index > lastIndex) {
        parts.push(text.substring(lastIndex, index));
      }

      if (url) {
        // Check if URL is an image or video
        if (isImageUrl(url)) {
          mediaUrls.push(url);
          // Don't add the URL as text, we'll render it as media below
        } else if (isVideoUrl(url)) {
          mediaUrls.push(url);
          // Don't add the URL as text, we'll render it as media below
        } else {
          // Handle regular URLs
          parts.push(
            <a
              key={`url-${keyCounter++}`}
              href={url}
              target="_blank"
              rel="noopener noreferrer"
              className="text-blue-500 hover:underline break-all"
            >
              {url}
            </a>
          );
        }
      } else if (nostrPrefix && nostrData) {
        // Handle Nostr references
        try {
          const nostrId = `${nostrPrefix}${nostrData}`;
          const decoded = nip19.decode(nostrId);

          if (decoded.type === 'npub') {
            const pubkey = decoded.data;
            parts.push(
              <NostrMention key={`mention-${keyCounter++}`} pubkey={pubkey} />
            );
          } else if (decoded.type === 'nprofile') {
            const pubkey = (decoded.data as any).pubkey;
            parts.push(
              <NostrMention key={`mention-${keyCounter++}`} pubkey={pubkey} />
            );
          } else if ((decoded.type === 'note' || decoded.type === 'nevent') && depth === 0) {
            // Store note reference for embedding later
            noteRefs.push({ index: parts.length, ref: nostrId });
            parts.push(null); // Placeholder
          } else {
            // For other types or nested notes, just show as a link
            parts.push(
              <Link
                key={`nostr-${keyCounter++}`}
                to={`/${nostrId}`}
                className="text-blue-500 hover:underline"
              >
                {fullMatch}
              </Link>
            );
          }
        } catch {
          // If decoding fails, just render as text
          parts.push(fullMatch);
        }
      } else if (hashtag) {
        // Handle hashtags
        const tag = hashtag.slice(1); // Remove the #
        parts.push(
          <Link
            key={`hashtag-${keyCounter++}`}
            to={`/t/${tag}`}
            className="text-blue-500 hover:underline"
          >
            {hashtag}
          </Link>
        );
      }

      lastIndex = index + fullMatch.length;
    }

    // Add any remaining text
    if (lastIndex < text.length) {
      parts.push(text.substring(lastIndex));
    }

    // If no special content was found, just use the plain text
    if (parts.length === 0 && mediaUrls.length === 0) {
      parts.push(text);
    }

    return { parts, mediaUrls, noteRefs };
  }, [event, depth]);

  // Replace note placeholders with embedded cards
  const finalParts = useMemo(() => {
    if (depth > 0 || !referencedEvents || Object.keys(referencedEvents).length === 0) {
      return content.parts.filter(p => p !== null);
    }

    const result = [...content.parts];

    for (const { index, ref } of content.noteRefs) {
      try {
        const decoded = nip19.decode(ref);
        const eventId = decoded.type === 'note' ? decoded.data as string : (decoded.data as any).id;
        const referencedEvent = referencedEvents[eventId];

        if (referencedEvent) {
          result[index] = <EmbeddedNote key={`embed-${ref}`} event={referencedEvent} />;
        } else {
          // If event not found, show as link
          result[index] = (
            <Link
              key={`nostr-${ref}`}
              to={`/${ref}`}
              className="text-blue-500 hover:underline"
            >
              nostr:{ref}
            </Link>
          );
        }
      } catch (error) {
        console.error('Failed to embed note:', error);
      }
    }

    return result.filter(p => p !== null);
  }, [content.parts, content.noteRefs, referencedEvents, depth]);

  // Get media from NIP-94 'imeta' tags if present
  const imetaTags = event.tags.filter(tag => tag[0] === 'imeta');
  const nip94MediaUrls = imetaTags
    .map(tag => {
      // Parse imeta tag format: ["imeta", "url https://...", "m image/jpeg", ...]
      const urlParam = tag.find(param => param.startsWith('url '));
      return urlParam ? urlParam.substring(4) : null;
    })
    .filter(Boolean) as string[];

  // Combine and deduplicate media URLs
  const allMediaUrls = Array.from(new Set([...content.mediaUrls, ...nip94MediaUrls]));

  return (
    <div className={cn("break-words", className)}>
      {/* Text content */}
      {finalParts.length > 0 && (
        <div className="whitespace-pre-wrap">
          {finalParts}
        </div>
      )}

      {/* Media attachments */}
      {allMediaUrls.length > 0 && (
        <div className="mt-2 space-y-2">
          {allMediaUrls.map((mediaUrl, index) => {
            if (isImageUrl(mediaUrl)) {
              return <MediaImage key={`media-${index}`} src={mediaUrl} alt="Post image" />;
            } else if (isVideoUrl(mediaUrl)) {
              return <MediaVideo key={`media-${index}`} src={mediaUrl} />;
            }
            return null;
          })}
        </div>
      )}
    </div>
  );
}

// Helper component to display user mentions
function NostrMention({ pubkey }: { pubkey: string }) {
  const { data: author, isLoading } = useAuthor(pubkey);
  const npub = nip19.npubEncode(pubkey);
  const hasRealName = !!author?.metadata?.name;
  const displayName = author?.metadata?.name ?? genUserName(pubkey);

  // Show loading state briefly
  if (isLoading) {
    return (
      <span className="text-muted-foreground">
        @...
      </span>
    );
  }

  return (
    <Link
      to={`/${npub}`}
      className={cn(
        "font-semibold hover:underline",
        hasRealName
          ? "text-blue-500"
          : "text-muted-foreground hover:text-foreground"
      )}
      onClick={(e) => e.stopPropagation()}
    >
      @{displayName}
    </Link>
  );
}
