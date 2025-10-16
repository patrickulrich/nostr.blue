import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { useDVMs } from '@/hooks/useDVMs';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Loader2, Zap, ExternalLink, Globe, Smartphone, Monitor, ArrowRight } from 'lucide-react';
import { nip19 } from 'nostr-tools';

// Map kind numbers to human-readable names
const kindNames: Record<number, string> = {
  5000: 'Text Processing',
  5001: 'Text-to-Speech',
  5002: 'Speech-to-Text',
  5003: 'Translation',
  5004: 'Summarization',
  5005: 'Translation',
  5006: 'Text Extraction',
  5050: 'Search',
  5100: 'Geohashing',
  5200: 'Content Discovery',
  5250: 'User Discovery',
  5300: 'Content Discovery',
  5301: 'Geohashing',
  5302: 'Discovery',
};

const platformIcons: Record<string, typeof Globe> = {
  web: Globe,
  ios: Smartphone,
  android: Smartphone,
  desktop: Monitor,
};

export function DVMPage() {
  useSeoMeta({
    title: 'Data Vending Machines / nostr.blue',
    description: 'Discover AI-powered services on Nostr',
  });

  const { dvms, isLoading } = useDVMs();
  const [selectedCategory, setSelectedCategory] = useState<string>('all');

  // Get unique categories from DVMs
  const categories = ['all', ...new Set(dvms.flatMap(dvm => dvm.tags))];

  // Filter DVMs by category
  let filteredDVMs = selectedCategory === 'all'
    ? dvms
    : dvms.filter(dvm => dvm.tags.includes(selectedCategory));

  // Sort to show feed-capable DVMs first (those with kinds 5050, 5200, 5250, 5300)
  filteredDVMs = filteredDVMs.sort((a, b) => {
    const aHasFeed = a.supportedKinds.some(k => [5050, 5200, 5250, 5300].includes(k));
    const bHasFeed = b.supportedKinds.some(k => [5050, 5200, 5250, 5300].includes(k));

    if (aHasFeed && !bHasFeed) return -1;
    if (!aHasFeed && bHasFeed) return 1;
    return 0;
  });

  return (
    <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
      <div className="min-h-screen">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
          <div className="p-4">
            <h1 className="text-xl font-bold flex items-center gap-2 mb-3">
              <Zap className="h-5 w-5 text-blue-500" />
              Data Vending Machines
            </h1>
            <p className="text-sm text-muted-foreground mb-3">
              Discover AI-powered services that can process your content
            </p>

            {/* Category filters */}
            <div className="flex gap-2 overflow-x-auto pb-2 -mb-2">
              {categories.map(category => (
                <Button
                  key={category}
                  variant={selectedCategory === category ? 'default' : 'outline'}
                  size="sm"
                  onClick={() => setSelectedCategory(category)}
                  className="flex-shrink-0"
                >
                  {category}
                </Button>
              ))}
            </div>
          </div>
        </div>

        {/* Content */}
        {isLoading ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : filteredDVMs.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <Zap className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">No DVMs Found</h2>
            <p className="text-muted-foreground max-w-sm">
              {selectedCategory === 'all'
                ? 'No Data Vending Machines are currently available. Connect to more relays to discover services.'
                : `No DVMs found for category "${selectedCategory}"`}
            </p>
          </div>
        ) : (
          <div className="p-4 space-y-4">
            {filteredDVMs.map((dvm) => {
              const npub = nip19.npubEncode(dvm.pubkey);

              return (
                <Card key={dvm.id} className="hover:shadow-md transition-shadow">
                  <CardHeader>
                    <div className="flex items-start gap-3">
                      <Avatar className="w-12 h-12">
                        <AvatarImage src={dvm.picture} alt={dvm.name || 'DVM'} />
                        <AvatarFallback>
                          <Zap className="h-6 w-6" />
                        </AvatarFallback>
                      </Avatar>
                      <div className="flex-1 min-w-0">
                        <CardTitle className="text-lg">
                          {dvm.name || 'Unnamed DVM'}
                        </CardTitle>
                        <CardDescription className="text-sm text-muted-foreground mt-1">
                          {npub.slice(0, 16)}...
                        </CardDescription>
                      </div>
                    </div>
                  </CardHeader>
                  <CardContent className="space-y-3">
                    {dvm.about && (
                      <p className="text-sm text-muted-foreground">
                        {dvm.about}
                      </p>
                    )}

                    {/* Supported Kinds */}
                    {dvm.supportedKinds.length > 0 && (
                      <div>
                        <h4 className="text-xs font-semibold text-muted-foreground uppercase mb-2">
                          Supported Services
                        </h4>
                        <div className="flex flex-wrap gap-2">
                          {dvm.supportedKinds.map(kind => (
                            <Badge key={kind} variant="secondary">
                              {kindNames[kind] || `Kind ${kind}`}
                            </Badge>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* Tags */}
                    {dvm.tags.length > 0 && (
                      <div>
                        <h4 className="text-xs font-semibold text-muted-foreground uppercase mb-2">
                          Topics
                        </h4>
                        <div className="flex flex-wrap gap-2">
                          {dvm.tags.map(tag => (
                            <Badge key={tag} variant="outline">
                              #{tag}
                            </Badge>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* Handlers/Platforms */}
                    {dvm.handlers.length > 0 && (
                      <div>
                        <h4 className="text-xs font-semibold text-muted-foreground uppercase mb-2">
                          Available On
                        </h4>
                        <div className="flex flex-wrap gap-2">
                          {dvm.handlers.map((handler, idx) => {
                            const Icon = platformIcons[handler.platform] || Globe;
                            return (
                              <Button
                                key={idx}
                                variant="outline"
                                size="sm"
                                className="gap-2"
                                onClick={() => {
                                  // For now, just log; in a real app, would redirect with proper entity
                                  console.log('Open DVM handler:', handler.url);
                                }}
                              >
                                <Icon className="h-4 w-4" />
                                {handler.platform}
                                <ExternalLink className="h-3 w-3" />
                              </Button>
                            );
                          })}
                        </div>
                      </div>
                    )}

                    {/* View Feed Button */}
                    {dvm.supportedKinds.some(k => [5050, 5200, 5250, 5300].includes(k)) && (
                      <div className="pt-3 border-t">
                        <Link to={`/dvm/${dvm.id}`}>
                          <Button className="w-full gap-2" variant="default">
                            <Zap className="h-4 w-4" />
                            View Feed
                            <ArrowRight className="h-4 w-4 ml-auto" />
                          </Button>
                        </Link>
                      </div>
                    )}
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

export default DVMPage;
