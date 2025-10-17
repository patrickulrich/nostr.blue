import { Search, TrendingUp, Heart, MessageCircle, Loader2 } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useState } from 'react';
import { useNavigate, Link } from 'react-router-dom';
import { useTrendingNotes } from '@/hooks/useTrendingNotes';
import { nip19 } from 'nostr-tools';

/**
 * Right sidebar component displaying search and trending posts from Nostr.Band.
 * Includes a search input and a scrollable list of trending notes.
 */
export function RightSidebar() {
  const [searchQuery, setSearchQuery] = useState('');
  const navigate = useNavigate();
  const { data: trendingNotes = [], isLoading } = useTrendingNotes(10);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (searchQuery.trim()) {
      navigate(`/search?q=${encodeURIComponent(searchQuery)}`);
    }
  };

  const truncateContent = (content: string, maxLength: number = 80) => {
    if (content.length <= maxLength) return content;
    return content.slice(0, maxLength).trim() + '...';
  };

  return (
    <div className="flex flex-col gap-4 sticky top-0 pt-2 pb-2 h-screen overflow-hidden">
      {/* Search Bar */}
      <form onSubmit={handleSearch} className="relative flex-shrink-0">
        <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 h-5 w-5 text-muted-foreground" />
        <Input
          type="text"
          placeholder="Search"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="pl-11 bg-muted/50 border-none rounded-full h-11 focus-visible:ring-1 focus-visible:ring-blue-500"
        />
      </form>

      {/* Nostr.Band Trending */}
      <Card className="border-border flex-1 flex flex-col overflow-hidden">
        <CardHeader className="pb-3 flex-shrink-0">
          <CardTitle className="text-xl flex items-center gap-2">
            <TrendingUp className="h-5 w-5" />
            Nostr.Band Trending
          </CardTitle>
        </CardHeader>
        <CardContent className="p-0 flex-1 flex flex-col overflow-hidden">
          {isLoading ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="h-6 w-6 animate-spin text-blue-500" />
            </div>
          ) : trendingNotes.length === 0 ? (
            <div className="px-4 py-8 text-center text-sm text-muted-foreground">
              No trending posts right now
            </div>
          ) : (
            <>
              <div className="flex-1 overflow-hidden">
                {trendingNotes.map((note) => {
                const noteId = nip19.noteEncode(note.event.id);
                const npub = nip19.npubEncode(note.event.pubkey);
                const authorName = note.profile?.display_name || note.profile?.name || npub.slice(0, 12);

                return (
                  <Link
                    key={note.event.id}
                    to={`/${noteId}`}
                    className="block px-4 py-3 hover:bg-accent/50 transition-colors border-b border-border last:border-0"
                  >
                    <div className="flex gap-3">
                      <img
                        src={note.profile?.picture || `https://api.dicebear.com/7.x/identicon/svg?seed=${note.event.pubkey}`}
                        alt={authorName}
                        className="w-10 h-10 rounded-full flex-shrink-0"
                      />
                      <div className="flex-1 min-w-0">
                        <div className="text-sm font-semibold truncate mb-1">{authorName}</div>
                        <div className="text-sm mb-2 line-clamp-2">
                          {truncateContent(note.event.content, 100)}
                        </div>
                        <div className="flex items-center gap-3 text-xs text-muted-foreground">
                          {note.stats?.reactions && note.stats.reactions > 0 && (
                            <span className="flex items-center gap-1">
                              <Heart className="h-3 w-3" />
                              {note.stats.reactions}
                            </span>
                          )}
                          {note.stats?.replies && note.stats.replies > 0 && (
                            <span className="flex items-center gap-1">
                              <MessageCircle className="h-3 w-3" />
                              {note.stats.replies}
                            </span>
                          )}
                        </div>
                      </div>
                    </div>
                  </Link>
                );
              })}
              </div>
              <button
                className="w-full px-4 py-3 text-blue-500 hover:bg-accent/50 transition-colors text-left text-sm flex-shrink-0 border-t border-border"
                onClick={() => navigate('/trending')}
              >
                Show more
              </button>
            </>
          )}
        </CardContent>
      </Card>

      {/* Footer Links */}
      <div className="px-4 text-xs text-muted-foreground flex flex-wrap gap-2 mt-auto flex-shrink-0">
        <a href="/terms" className="hover:underline">Terms of Service</a>
        <span>·</span>
        <a href="/privacy" className="hover:underline">Privacy Policy</a>
        <span>·</span>
        <a href="/cookies" className="hover:underline">Cookie Policy</a>
        <span>·</span>
        <a href="/about" className="hover:underline">About</a>
        <div className="w-full mt-1">© 2024 nostr.blue</div>
      </div>
    </div>
  );
}
