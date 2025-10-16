import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { useCommunities, useUserCommunities } from '@/hooks/useCommunities';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Loader2, Users, Search, ArrowRight } from 'lucide-react';

export function CommunitiesPage() {
  useSeoMeta({
    title: 'Communities / nostr.blue',
    description: 'Discover and join Nostr communities',
  });

  const { user } = useCurrentUser();
  const { data: communities, isLoading } = useCommunities();
  const { data: userCommunities } = useUserCommunities();
  const [searchQuery, setSearchQuery] = useState('');

  // Filter and sort communities
  const filteredCommunities = communities?.filter(community => {
    if (!searchQuery) return true;

    const query = searchQuery.toLowerCase();
    return (
      community.name?.toLowerCase().includes(query) ||
      community.description?.toLowerCase().includes(query) ||
      community.dTag.toLowerCase().includes(query)
    );
  }).sort((a, b) => {
    // Sort user's communities first (either posted to or moderating)
    const aIsMember = userCommunities?.has(a.aTag) || (user && a.moderators.includes(user.pubkey));
    const bIsMember = userCommunities?.has(b.aTag) || (user && b.moderators.includes(user.pubkey));

    if (aIsMember && !bIsMember) return -1;
    if (!aIsMember && bIsMember) return 1;

    // Then sort by created date
    return b.event.created_at - a.event.created_at;
  }) || [];

  return (
    <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
      <div className="min-h-screen">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
          <div className="p-4">
            <h1 className="text-xl font-bold flex items-center gap-2 mb-3">
              <Users className="h-5 w-5 text-blue-500" />
              Communities
            </h1>
            <p className="text-sm text-muted-foreground mb-3">
              Discover communities and join the conversation
            </p>

            {/* Search */}
            <div className="relative">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
              <Input
                placeholder="Search communities..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="pl-10"
              />
            </div>
          </div>
        </div>

        {/* Content */}
        {isLoading ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : filteredCommunities.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <Users className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">
              {searchQuery ? 'No communities found' : 'No communities available'}
            </h2>
            <p className="text-muted-foreground max-w-sm">
              {searchQuery
                ? 'Try a different search term'
                : 'Connect to more relays to discover communities'}
            </p>
          </div>
        ) : (
          <div className="p-4 space-y-4">
            {filteredCommunities.map((community) => {
              const isMember = userCommunities?.has(community.aTag) || (user && community.moderators.includes(user.pubkey));
              const isModerator = user && community.moderators.includes(user.pubkey);

              return (
                <Card key={community.id} className="hover:shadow-md transition-shadow">
                  <CardHeader>
                    <div className="flex items-start gap-3">
                      <Avatar className="w-12 h-12">
                        <AvatarImage src={community.image} alt={community.name || 'Community'} />
                        <AvatarFallback>
                          <Users className="h-6 w-6" />
                        </AvatarFallback>
                      </Avatar>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <CardTitle className="text-lg">
                            {community.name || 'Unnamed Community'}
                          </CardTitle>
                          {isModerator && (
                            <Badge variant="default" className="text-xs">
                              Moderator
                            </Badge>
                          )}
                          {isMember && !isModerator && (
                            <Badge variant="secondary" className="text-xs">
                              Member
                            </Badge>
                          )}
                        </div>
                        <CardDescription className="text-sm text-muted-foreground mt-1">
                          {community.dTag}
                        </CardDescription>
                      </div>
                    </div>
                  </CardHeader>
                  <CardContent className="space-y-3">
                    {community.description && (
                      <p className="text-sm text-muted-foreground">
                        {community.description}
                      </p>
                    )}

                    {/* Moderators */}
                    {community.moderators.length > 0 && (
                      <div>
                        <h4 className="text-xs font-semibold text-muted-foreground uppercase mb-2">
                          Moderators
                        </h4>
                        <p className="text-sm text-muted-foreground">
                          {community.moderators.length} moderator{community.moderators.length !== 1 ? 's' : ''}
                        </p>
                      </div>
                    )}

                    {/* View Community Button */}
                    <div className="pt-3 border-t">
                      <Link to={`/community/${encodeURIComponent(community.aTag)}`}>
                        <Button className="w-full gap-2" variant="default">
                          <Users className="h-4 w-4" />
                          View Community
                          <ArrowRight className="h-4 w-4 ml-auto" />
                        </Button>
                      </Link>
                    </div>
                  </CardContent>
                </Card>
              );
            })}
          </div>
        )}
      </div>
    </MainLayout>
  );
}

export default CommunitiesPage;
