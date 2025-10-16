import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { PostCard } from '@/components/PostCard';
import { useTrendingNotes } from '@/hooks/useTrendingNotes';
import { Loader2, TrendingUp } from 'lucide-react';

export function TrendingPage() {
  const { data: trendingNotes = [], isLoading } = useTrendingNotes();

  useSeoMeta({
    title: 'Trending / nostr.blue',
    description: 'Discover trending posts on Nostr',
  });

  return (
    <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
      <div className="min-h-screen">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
          <div className="flex items-center gap-4 p-4">
            <TrendingUp className="h-6 w-6" />
            <div>
              <h1 className="text-xl font-bold">Trending</h1>
              <p className="text-sm text-muted-foreground">
                Posts trending on Nostr in the last 24 hours
              </p>
            </div>
          </div>
        </div>

        {/* Content */}
        {isLoading ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : trendingNotes.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 px-4 text-center">
            <TrendingUp className="h-16 w-16 text-muted-foreground mb-4" />
            <h2 className="text-2xl font-bold mb-2">No trending posts</h2>
            <p className="text-muted-foreground max-w-sm">
              Check back later to see what's trending on Nostr.
            </p>
          </div>
        ) : (
          <>
            <div className="border-b border-border p-4 bg-muted/30">
              <p className="text-sm text-muted-foreground">
                Showing {trendingNotes.length} trending {trendingNotes.length === 1 ? 'post' : 'posts'} from{' '}
                <a
                  href="https://nostr.band"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-blue-500 hover:underline"
                >
                  Nostr.Band
                </a>
              </p>
            </div>
            {trendingNotes.map((note) => (
              <PostCard key={note.event.id} event={note.event} />
            ))}
          </>
        )}
      </div>
    </MainLayout>
  );
}

export default TrendingPage;
