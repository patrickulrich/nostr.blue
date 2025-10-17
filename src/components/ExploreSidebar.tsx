import { Search, UserPlus, Loader2 } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { useState } from 'react';
import { useNavigate, Link } from 'react-router-dom';
import { useSuggestedProfiles } from '@/hooks/useSuggestedProfiles';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { FollowButton } from '@/components/FollowButton';
import { nip19 } from 'nostr-tools';
import { genUserName } from '@/lib/genUserName';

export function ExploreSidebar() {
  const [searchQuery, setSearchQuery] = useState('');
  const navigate = useNavigate();
  const { user } = useCurrentUser();
  const { data: suggestedProfiles = [], isLoading, error } = useSuggestedProfiles(user?.pubkey, 5);

  console.log('[ExploreSidebar] User pubkey:', user?.pubkey);
  console.log('[ExploreSidebar] Is loading:', isLoading);
  console.log('[ExploreSidebar] Error:', error);
  console.log('[ExploreSidebar] Suggested profiles:', suggestedProfiles);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (searchQuery.trim()) {
      navigate(`/search?q=${encodeURIComponent(searchQuery)}`);
    }
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

      {/* Suggested Profiles */}
      <Card className="border-border flex-1 flex flex-col overflow-hidden">
        <CardHeader className="pb-3 flex-shrink-0">
          <CardTitle className="text-xl flex items-center gap-2">
            <UserPlus className="h-5 w-5" />
            Who to follow
          </CardTitle>
        </CardHeader>
        <CardContent className="p-0 flex-1 flex flex-col overflow-hidden">
          {!user ? (
            <div className="px-4 py-8 text-center text-sm text-muted-foreground">
              Log in to see suggestions
            </div>
          ) : isLoading ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="h-6 w-6 animate-spin text-blue-500" />
            </div>
          ) : suggestedProfiles.length === 0 ? (
            <div className="px-4 py-8 text-center text-sm text-muted-foreground">
              No suggestions available
            </div>
          ) : (
              <div className="flex-1 overflow-y-auto">
                {suggestedProfiles.map((profile) => {
                  const npub = nip19.npubEncode(profile.pubkey);
                  const displayName = profile.profile?.display_name || profile.profile?.name || genUserName(profile.pubkey);
                  const username = profile.profile?.name || npub.slice(0, 12);
                  const avatarUrl = profile.profile?.picture;

                  return (
                    <div
                      key={profile.pubkey}
                      className="px-4 py-3 hover:bg-accent/50 transition-colors border-b border-border last:border-0"
                    >
                      <div className="flex items-start gap-3">
                        <Link to={`/${npub}`} className="flex-shrink-0">
                          <Avatar className="w-12 h-12">
                            <AvatarImage src={avatarUrl} alt={displayName} />
                            <AvatarFallback>{displayName[0]?.toUpperCase() || 'A'}</AvatarFallback>
                          </Avatar>
                        </Link>
                        <div className="flex-1 min-w-0">
                          <Link to={`/${npub}`} className="block">
                            <div className="text-sm font-semibold truncate hover:underline">
                              {displayName}
                            </div>
                            <div className="text-sm text-muted-foreground truncate">
                              @{username}
                            </div>
                          </Link>
                          {profile.profile?.about && (
                            <p className="text-sm text-muted-foreground line-clamp-2 mt-1">
                              {profile.profile.about}
                            </p>
                          )}
                        </div>
                        <FollowButton pubkey={profile.pubkey} className="px-4" />
                      </div>
                    </div>
                  );
                })}
              </div>
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
