import { useSeoMeta } from '@unhead/react';
import { MainLayout } from '@/components/MainLayout';
import { AppSidebar } from '@/components/AppSidebar';
import { RightSidebar } from '@/components/RightSidebar';
import { useSettings } from '@/hooks/useSettings';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Loader2, Moon, Sun, Monitor } from 'lucide-react';
import { useToast } from '@/hooks/useToast';

export function SettingsPage() {
  useSeoMeta({
    title: 'Settings / nostr.blue',
    description: 'Manage your nostr.blue settings',
  });

  const { user } = useCurrentUser();
  const { settings, isLoading, updateSettings } = useSettings();
  const { toast } = useToast();

  const handleThemeChange = async (theme: 'light' | 'dark' | 'system') => {
    try {
      await updateSettings.mutateAsync({ theme });
      toast({
        title: 'Settings saved',
        description: `Theme updated to ${theme} mode.`,
      });
    } catch (error) {
      console.error('Failed to update theme:', error);
      toast({
        title: 'Error',
        description: 'Failed to save settings. Please try again.',
        variant: 'destructive',
      });
    }
  };

  if (!user) {
    return (
      <MainLayout sidebar={<AppSidebar />} rightPanel={<RightSidebar />}>
        <div className="min-h-screen">
          <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
            <div className="p-4">
              <h1 className="text-xl font-bold">Settings</h1>
            </div>
          </div>
          <div className="p-8 text-center">
            <Card>
              <CardContent className="py-12">
                <h3 className="text-lg font-semibold mb-2">Login Required</h3>
                <p className="text-muted-foreground">
                  You need to be logged in to access settings.
                </p>
              </CardContent>
            </Card>
          </div>
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
            <h1 className="text-xl font-bold">Settings</h1>
            <p className="text-sm text-muted-foreground mt-1">
              Manage your preferences. Settings are saved to Nostr.
            </p>
          </div>
        </div>

        {/* Content */}
        {isLoading ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
          </div>
        ) : (
          <div className="p-4 space-y-4 max-w-2xl">
            {/* Appearance Settings */}
            <Card>
              <CardHeader>
                <CardTitle>Appearance</CardTitle>
                <CardDescription>
                  Customize how nostr.blue looks on your device
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="space-y-3">
                  <Label className="text-base font-semibold">Theme</Label>
                  <RadioGroup
                    value={settings.theme}
                    onValueChange={(value) => handleThemeChange(value as 'light' | 'dark' | 'system')}
                    disabled={updateSettings.isPending}
                    className="space-y-3"
                  >
                    <div className="flex items-center space-x-3 p-3 rounded-lg border hover:bg-accent cursor-pointer">
                      <RadioGroupItem value="light" id="theme-light" />
                      <Label
                        htmlFor="theme-light"
                        className="flex items-center gap-3 cursor-pointer flex-1"
                      >
                        <Sun className="h-5 w-5" />
                        <div>
                          <div className="font-medium">Light</div>
                          <div className="text-sm text-muted-foreground">
                            Always use light theme
                          </div>
                        </div>
                      </Label>
                    </div>

                    <div className="flex items-center space-x-3 p-3 rounded-lg border hover:bg-accent cursor-pointer">
                      <RadioGroupItem value="dark" id="theme-dark" />
                      <Label
                        htmlFor="theme-dark"
                        className="flex items-center gap-3 cursor-pointer flex-1"
                      >
                        <Moon className="h-5 w-5" />
                        <div>
                          <div className="font-medium">Dark</div>
                          <div className="text-sm text-muted-foreground">
                            Always use dark theme
                          </div>
                        </div>
                      </Label>
                    </div>

                    <div className="flex items-center space-x-3 p-3 rounded-lg border hover:bg-accent cursor-pointer">
                      <RadioGroupItem value="system" id="theme-system" />
                      <Label
                        htmlFor="theme-system"
                        className="flex items-center gap-3 cursor-pointer flex-1"
                      >
                        <Monitor className="h-5 w-5" />
                        <div>
                          <div className="font-medium">System</div>
                          <div className="text-sm text-muted-foreground">
                            Use system theme
                          </div>
                        </div>
                      </Label>
                    </div>
                  </RadioGroup>
                </div>

                {updateSettings.isPending && (
                  <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Loader2 className="h-4 w-4 animate-spin" />
                    Saving to Nostr...
                  </div>
                )}
              </CardContent>
            </Card>

            {/* Account Information */}
            <Card>
              <CardHeader>
                <CardTitle>Account</CardTitle>
                <CardDescription>
                  Your Nostr account information
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                <div>
                  <Label className="text-sm text-muted-foreground">Public Key</Label>
                  <p className="text-sm font-mono mt-1 break-all">
                    {user.pubkey}
                  </p>
                </div>
              </CardContent>
            </Card>

            {/* About */}
            <Card>
              <CardHeader>
                <CardTitle>About</CardTitle>
                <CardDescription>
                  Information about nostr.blue
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                <div>
                  <Label className="text-sm text-muted-foreground">Version</Label>
                  <p className="text-sm mt-1">1.0.0</p>
                </div>
                <div>
                  <Label className="text-sm text-muted-foreground">Settings Storage</Label>
                  <p className="text-sm mt-1">
                    Settings are stored using NIP-78 (kind 30078) and synced across all your devices via Nostr relays.
                  </p>
                </div>
              </CardContent>
            </Card>
          </div>
        )}
      </div>
    </MainLayout>
  );
}

export default SettingsPage;
