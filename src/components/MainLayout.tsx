import { ReactNode } from 'react';
import { cn } from '@/lib/utils';

interface MainLayoutProps {
  children: ReactNode;
  sidebar?: ReactNode;
  rightPanel?: ReactNode;
  className?: string;
}

export function MainLayout({ children, sidebar, rightPanel, className }: MainLayoutProps) {
  return (
    <div className="min-h-screen bg-background">
      <div className="max-w-[1280px] mx-auto flex">
        {/* Left Sidebar - Navigation */}
        {sidebar && (
          <aside className="hidden lg:flex lg:w-[275px] flex-shrink-0 sticky top-0 h-screen border-r border-border">
            <div className="flex flex-col w-full p-4">
              {sidebar}
            </div>
          </aside>
        )}

        {/* Main Content */}
        <main className={cn("flex-1 min-w-0 border-r border-border", className)}>
          {children}
        </main>

        {/* Right Panel - Trends, Suggestions, etc. */}
        {rightPanel && (
          <aside className="hidden xl:flex xl:w-[350px] flex-shrink-0 sticky top-0 h-screen">
            <div className="flex flex-col w-full p-4">
              {rightPanel}
            </div>
          </aside>
        )}
      </div>
    </div>
  );
}
