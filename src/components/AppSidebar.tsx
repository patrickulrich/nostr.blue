import { Home, Compass, User, Settings, PenSquare, Bell, Mail, List, Bookmark, Users, MoreHorizontal, Video, Calendar, Music, Zap } from 'lucide-react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { LoginArea } from '@/components/auth/LoginArea';
import { PostComposer } from '@/components/PostComposer';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { nip19 } from 'nostr-tools';
import { cn } from '@/lib/utils';
import { useState } from 'react';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { useQueryClient } from '@tanstack/react-query';

interface NavItemProps {
  to: string;
  icon: React.ReactNode;
  label: string;
  active?: boolean;
}

function NavItem({ to, icon, label, active }: NavItemProps) {
  return (
    <Link to={to}>
      <Button
        variant="ghost"
        className={cn(
          "w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent",
          active && "font-bold"
        )}
      >
        {icon}
        <span className="hidden xl:inline">{label}</span>
      </Button>
    </Link>
  );
}

/**
 * Main application sidebar with navigation links and post composer.
 * Shows different navigation items based on user authentication status.
 */
export function AppSidebar() {
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { user } = useCurrentUser();
  const [composeOpen, setComposeOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);

  const profilePath = user ? `/${nip19.npubEncode(user.pubkey)}` : '/';

  const handleHomeClick = (e: React.MouseEvent) => {
    // If already on home page, scroll to top and refresh feed
    if (location.pathname === '/') {
      e.preventDefault();
      window.scrollTo({ top: 0, behavior: 'smooth' });
      // Invalidate feed queries to trigger a refresh
      queryClient.invalidateQueries({ queryKey: ['feed'] });
      queryClient.invalidateQueries({ queryKey: ['popular-feed-events'] });
    } else {
      // Otherwise navigate normally (Link will handle it)
      navigate('/');
    }
  };

  return (
    <div className="flex flex-col h-full justify-between">
      <div className="flex flex-col gap-2">
        {/* Logo/Brand */}
        <div className="px-4 py-3 mb-2">
          <Link to="/" onClick={handleHomeClick}>
            <div className="w-12 h-12 rounded-full bg-blue-500 flex items-center justify-center text-white font-bold text-xl hover:bg-blue-600 transition-colors">
              N
            </div>
          </Link>
        </div>

        {/* Navigation */}
        <nav className="flex flex-col gap-1">
          {/* Home - with special click handling for refresh */}
          <Link to="/" onClick={handleHomeClick}>
            <Button
              variant="ghost"
              className={cn(
                "w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent",
                location.pathname === '/' && "font-bold"
              )}
            >
              <Home className="w-7 h-7" />
              <span className="hidden xl:inline">Home</span>
            </Button>
          </Link>
          <NavItem
            to="/explore"
            icon={<Compass className="w-7 h-7" />}
            label="Explore"
            active={location.pathname === '/explore'}
          />
          {user && (
            <>
              <NavItem
                to="/notifications"
                icon={<Bell className="w-7 h-7" />}
                label="Notifications"
                active={location.pathname === '/notifications'}
              />
              <NavItem
                to="/messages"
                icon={<Mail className="w-7 h-7" />}
                label="Messages"
                active={location.pathname === '/messages'}
              />
              <NavItem
                to="/dvm"
                icon={<Zap className="w-7 h-7" />}
                label="DVM"
                active={location.pathname === '/dvm'}
              />
              <NavItem
                to="/lists"
                icon={<List className="w-7 h-7" />}
                label="Lists"
                active={location.pathname === '/lists'}
              />
              <NavItem
                to="/bookmarks"
                icon={<Bookmark className="w-7 h-7" />}
                label="Bookmarks"
                active={location.pathname === '/bookmarks'}
              />
              <NavItem
                to="/communities"
                icon={<Users className="w-7 h-7" />}
                label="Communities"
                active={location.pathname === '/communities'}
              />
              <NavItem
                to={profilePath}
                icon={<User className="w-7 h-7" />}
                label="Profile"
                active={location.pathname === profilePath}
              />
              <NavItem
                to="/settings"
                icon={<Settings className="w-7 h-7" />}
                label="Settings"
                active={location.pathname === '/settings'}
              />
            </>
          )}
          <Popover open={moreOpen} onOpenChange={setMoreOpen}>
            <PopoverTrigger asChild>
              <Button
                variant="ghost"
                className="w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent"
              >
                <MoreHorizontal className="w-7 h-7" />
                <span className="hidden xl:inline">More</span>
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-80 p-0" align="start" side="top">
              <div className="flex flex-col">
                <a href="https://vlogstr.com" target="_blank" rel="noopener noreferrer" onClick={() => setMoreOpen(false)}>
                  <Button
                    variant="ghost"
                    className="w-full justify-start gap-4 text-base py-4 px-4 rounded-none hover:bg-accent"
                  >
                    <Video className="w-5 h-5" />
                    <span>Vlogstr</span>
                  </Button>
                </a>
                <a href="https://nostrcal.com" target="_blank" rel="noopener noreferrer" onClick={() => setMoreOpen(false)}>
                  <Button
                    variant="ghost"
                    className="w-full justify-start gap-4 text-base py-4 px-4 rounded-none hover:bg-accent"
                  >
                    <Calendar className="w-5 h-5" />
                    <span>nostrcal</span>
                  </Button>
                </a>
                <a href="https://nostrmusic.com" target="_blank" rel="noopener noreferrer" onClick={() => setMoreOpen(false)}>
                  <Button
                    variant="ghost"
                    className="w-full justify-start gap-4 text-base py-4 px-4 rounded-none hover:bg-accent"
                  >
                    <Music className="w-5 h-5" />
                    <span>nostrmusic</span>
                  </Button>
                </a>
              </div>
            </PopoverContent>
          </Popover>
        </nav>

        {/* Post Button */}
        {user && (
          <div className="mt-4 px-2">
            <Button
              onClick={() => setComposeOpen(true)}
              className="w-full rounded-full py-6 text-lg font-bold"
              size="lg"
            >
              <PenSquare className="w-6 h-6 xl:mr-2" />
              <span className="hidden xl:inline">Post</span>
            </Button>
          </div>
        )}
      </div>

      {/* Login/Account Section */}
      <div className="mt-auto px-2 pb-4">
        <LoginArea />
      </div>

      {/* Compose Dialog */}
      <Dialog open={composeOpen} onOpenChange={setComposeOpen}>
        <DialogContent className="max-w-2xl">
          <PostComposer
            onSuccess={() => setComposeOpen(false)}
            autoFocus={true}
          />
        </DialogContent>
      </Dialog>
    </div>
  );
}
